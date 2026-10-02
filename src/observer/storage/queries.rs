use super::{
    Aggregate, CallRecord, History, MinuteAggregate, Origin, OriginAggregate, Outcome,
    ToolAggregate, sql_error,
};
use rusqlite::{Connection, Row, params};
use std::collections::BTreeMap;
use std::io;
use std::sync::OnceLock;

pub(super) fn append(connection: &Connection, record: &CallRecord) -> io::Result<()> {
    validate(record)?;
    connection.execute(
        "INSERT INTO completed_calls (sequence, started_at_ms, completed_at_ms, tool, outcome, duration_ms, request_bytes, response_bytes, source, evidence, transport, session_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![record.sequence as i64, record.started_at_ms as i64, record.completed_at_ms as i64,
            normalize_tool(&record.tool), outcome_name(record.outcome), record.duration_ms,
            record.request_bytes as i64, record.response_bytes as i64,
            record.origin.source.as_str(), record.origin.attribution.as_str(), record.origin.transport.as_str(),
            record.session_id as i64],
    ).map_err(sql_error)?;
    Ok(())
}

pub(super) fn load(connection: &Connection, now_ms: u64) -> io::Result<History> {
    let transaction = connection.unchecked_transaction().map_err(sql_error)?;
    let (totals, request_bytes, response_bytes, last_sequence, last_session_id) =
        totals(&transaction)?;
    let history = History {
        totals,
        request_bytes,
        response_bytes,
        last_sequence,
        last_session_id,
        tools: tools(&transaction)?,
        origins: origins(&transaction)?,
        recent_calls: recent_calls(&transaction)?,
        minutes: minutes(&transaction, now_ms)?,
    };
    transaction.commit().map_err(sql_error)?;
    Ok(history)
}

fn validate(record: &CallRecord) -> io::Result<()> {
    let numbers = [
        record.sequence,
        record.started_at_ms,
        record.completed_at_ms,
        record.request_bytes,
        record.response_bytes,
        record.session_id,
    ];
    if record.sequence == 0
        || numbers.iter().any(|value| *value > i64::MAX as u64)
        || !record.duration_ms.is_finite()
        || record.duration_ms < 0.0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Observer call metadata is outside supported numeric ranges",
        ));
    }
    Ok(())
}

fn totals(connection: &Connection) -> io::Result<(Aggregate, u64, u64, u64, u64)> {
    connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(outcome = 'success'), 0), COALESCE(SUM(outcome != 'success'), 0),
         COALESCE(SUM(duration_ms), 0), COALESCE(SUM(request_bytes), 0), COALESCE(SUM(response_bytes), 0),
         COALESCE(MAX(sequence), 0), COALESCE(MAX(session_id), 0) FROM completed_calls",
        [], |row| Ok((aggregate(row, 0)?, counter(row, 4)?, counter(row, 5)?, counter(row, 6)?, counter(row, 7)?)),
    ).map_err(sql_error)
}

fn tools(connection: &Connection) -> io::Result<Vec<ToolAggregate>> {
    let mut statement = connection.prepare(
        "SELECT tool, COUNT(*), SUM(outcome = 'success'), SUM(outcome != 'success'), SUM(duration_ms),
         MAX(started_at_ms) FROM completed_calls GROUP BY tool ORDER BY tool").map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(ToolAggregate {
                tool: normalize_tool(&row.get::<_, String>(0)?).into(),
                totals: aggregate(row, 1)?,
                last_called_at_ms: counter(row, 5)?,
            })
        })
        .map_err(sql_error)?;
    let mut normalized = BTreeMap::<String, ToolAggregate>::new();
    for row in rows {
        merge_tool(&mut normalized, row.map_err(sql_error)?);
    }
    Ok(normalized.into_values().collect())
}

fn merge_tool(tools: &mut BTreeMap<String, ToolAggregate>, incoming: ToolAggregate) {
    let tool = tools
        .entry(incoming.tool.clone())
        .or_insert_with(|| ToolAggregate {
            tool: incoming.tool.clone(),
            totals: Aggregate::default(),
            last_called_at_ms: 0,
        });
    merge_totals(&mut tool.totals, &incoming.totals);
    tool.last_called_at_ms = tool.last_called_at_ms.max(incoming.last_called_at_ms);
}

fn origins(connection: &Connection) -> io::Result<Vec<OriginAggregate>> {
    let mut statement = connection.prepare(
        "SELECT source, evidence, transport, COUNT(*), SUM(outcome = 'success'), SUM(outcome != 'success'), SUM(duration_ms)
         FROM completed_calls GROUP BY source, evidence, transport ORDER BY source, evidence, transport").map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(OriginAggregate {
                origin: origin(row, 0)?,
                totals: aggregate(row, 3)?,
            })
        })
        .map_err(sql_error)?;
    let mut normalized = BTreeMap::<Origin, Aggregate>::new();
    for row in rows {
        let incoming = row.map_err(sql_error)?;
        merge_totals(
            normalized.entry(incoming.origin).or_default(),
            &incoming.totals,
        );
    }
    Ok(normalized
        .into_iter()
        .map(|(origin, totals)| OriginAggregate { origin, totals })
        .collect())
}

fn merge_totals(totals: &mut Aggregate, incoming: &Aggregate) {
    totals.calls += incoming.calls;
    totals.successes += incoming.successes;
    totals.errors += incoming.errors;
    totals.duration_ms += incoming.duration_ms;
}

fn recent_calls(connection: &Connection) -> io::Result<Vec<CallRecord>> {
    let mut statement = connection.prepare(
        "SELECT sequence, started_at_ms, completed_at_ms, tool, outcome, duration_ms, request_bytes, response_bytes, source, evidence, transport, session_id
         FROM completed_calls ORDER BY id DESC LIMIT 1000").map_err(sql_error)?;
    let rows = statement.query_map([], call_record).map_err(sql_error)?;
    let mut calls = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    calls.reverse();
    Ok(calls)
}

fn call_record(row: &Row<'_>) -> rusqlite::Result<CallRecord> {
    let outcome: String = row.get(4)?;
    let outcome = match outcome.as_str() {
        "success" => Outcome::Success,
        "tool_error" => Outcome::ToolError,
        "protocol_error" => Outcome::ProtocolError,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(CallRecord {
        sequence: counter(row, 0)?,
        started_at_ms: counter(row, 1)?,
        completed_at_ms: counter(row, 2)?,
        tool: normalize_tool(&row.get::<_, String>(3)?).into(),
        outcome,
        duration_ms: row.get(5)?,
        request_bytes: counter(row, 6)?,
        response_bytes: counter(row, 7)?,
        origin: origin(row, 8)?,
        session_id: counter(row, 11)?,
    })
}

fn origin(row: &Row<'_>, offset: usize) -> rusqlite::Result<Origin> {
    Ok(Origin::from_storage(
        &row.get::<_, String>(offset)?,
        &row.get::<_, String>(offset + 1)?,
        &row.get::<_, String>(offset + 2)?,
    ))
}

fn minutes(connection: &Connection, now_ms: u64) -> io::Result<Vec<MinuteAggregate>> {
    let current = now_ms / 60_000 * 60_000;
    let oldest = current.saturating_sub(59 * 60_000);
    let upper = current.saturating_add(60_000).min(i64::MAX as u64);
    let mut statement = connection.prepare(
        "SELECT completed_at_ms / 60000 * 60000 AS minute, COUNT(*), SUM(outcome != 'success'), SUM(duration_ms)
         FROM completed_calls WHERE completed_at_ms >= ?1 AND completed_at_ms < ?2 GROUP BY minute ORDER BY minute"
    ).map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![oldest.min(i64::MAX as u64) as i64, upper as i64],
            |row| {
                Ok(MinuteAggregate {
                    minute_start_ms: counter(row, 0)?,
                    calls: counter(row, 1)?,
                    errors: counter(row, 2)?,
                    duration_ms: row.get(3)?,
                })
            },
        )
        .map_err(sql_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)
}

fn aggregate(row: &Row<'_>, offset: usize) -> rusqlite::Result<Aggregate> {
    Ok(Aggregate {
        calls: counter(row, offset)?,
        successes: counter(row, offset + 1)?,
        errors: counter(row, offset + 2)?,
        duration_ms: row.get(offset + 3)?,
    })
}

fn counter(row: &Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn normalize_tool(name: &str) -> &'static str {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            crate::tools::get_available_tools()
                .into_iter()
                .map(|tool| tool.name)
                .collect()
        })
        .iter()
        .find(|known| **known == name)
        .copied()
        .unwrap_or("unknown_tool")
}

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Success => "success",
        Outcome::ToolError => "tool_error",
        Outcome::ProtocolError => "protocol_error",
    }
}
