use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

pub struct McpClient {
    child: std::process::Child,
    reader: BufReader<std::process::ChildStdout>,
}

#[allow(dead_code)]
impl McpClient {
    pub fn spawn(root_dir: &std::path::Path) -> Self {
        Self::spawn_with_options(&["--root", root_dir.to_str().unwrap()], &[], None)
    }

    pub fn spawn_with_options(
        args: &[&str],
        env: &[(&str, &str)],
        cwd: Option<&std::path::Path>,
    ) -> Self {
        let bin_path = env!("CARGO_BIN_EXE_file-system-mcp");
        let mut cmd = Command::new(bin_path);
        for arg in args {
            cmd.arg(arg);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn().expect("Failed to spawn file-system-mcp binary");
        let stdout = child.stdout.take().expect("Child must have stdout");
        let reader = BufReader::new(stdout);

        McpClient { child, reader }
    }

    pub fn send_request(&mut self, request: Value) -> Value {
        let stdin = self.child.stdin.as_mut().expect("Child must have stdin");
        let mut line = serde_json::to_string(&request).unwrap();
        line.push('\n');
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.flush().unwrap();

        let mut resp_line = String::new();
        self.reader
            .read_line(&mut resp_line)
            .expect("Failed to read response line");

        serde_json::from_str(&resp_line).unwrap_or_else(|e| {
            panic!(
                "Failed to parse response JSON: '{}', error: {}",
                resp_line, e
            )
        })
    }

    pub fn send_notification(&mut self, notification: Value) {
        let stdin = self.child.stdin.as_mut().expect("Child must have stdin");
        let mut line = serde_json::to_string(&notification).unwrap();
        line.push('\n');
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
