use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Unknown,
    Chatgpt,
    ChatgptWork,
    Codex,
    CodexCloud,
    OpenaiDot,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Attribution {
    #[default]
    Unknown,
    ClientReported,
    OperatorConfigured,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    #[default]
    Unknown,
    Stdio,
    Tunnel,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Origin {
    pub source: Source,
    pub attribution: Attribution,
    pub transport: Transport,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Context {
    pub source: Source,
    pub transport: Transport,
}

impl Source {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "unknown" => Some(Self::Unknown),
            "chatgpt" => Some(Self::Chatgpt),
            "chatgpt_work" | "chatgpt-work" => Some(Self::ChatgptWork),
            "codex" => Some(Self::Codex),
            "codex_cloud" | "codex-cloud" => Some(Self::CodexCloud),
            "openai_dot" | "openai-dot" => Some(Self::OpenaiDot),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Chatgpt => "chatgpt",
            Self::ChatgptWork => "chatgpt_work",
            Self::Codex => "codex",
            Self::CodexCloud => "codex_cloud",
            Self::OpenaiDot => "openai_dot",
        }
    }
}

impl Attribution {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::ClientReported => "client_reported",
            Self::OperatorConfigured => "operator_configured",
        }
    }
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Stdio => "stdio",
            Self::Tunnel => "tunnel",
        }
    }
}

impl Origin {
    pub fn from_storage(source: &str, attribution: &str, transport: &str) -> Self {
        let source = Source::parse(source).unwrap_or_default();
        let attribution = match attribution {
            "client_reported" if source != Source::Unknown => Attribution::ClientReported,
            "operator_configured" if source != Source::Unknown => Attribution::OperatorConfigured,
            _ => Attribution::Unknown,
        };
        let transport = match transport {
            "stdio" => Transport::Stdio,
            "tunnel" => Transport::Tunnel,
            _ => Transport::Unknown,
        };
        Self {
            source: if attribution == Attribution::Unknown {
                Source::Unknown
            } else {
                source
            },
            attribution,
            transport,
        }
    }

    pub fn for_call(params: Option<&Value>, configured: Source, transport: Transport) -> Self {
        let hint = params
            .and_then(|value| value.get("_meta"))
            .and_then(|value| value.get("file-system-mcp/source"));
        let (source, attribution) = match hint {
            Some(value) => (
                value.as_str().and_then(Source::parse).unwrap_or_default(),
                Attribution::ClientReported,
            ),
            None => (configured, Attribution::OperatorConfigured),
        };
        Self {
            source,
            attribution: if source == Source::Unknown {
                Attribution::Unknown
            } else {
                attribution
            },
            transport,
        }
    }
}

#[cfg(test)]
#[path = "origin_tests.rs"]
mod tests;
