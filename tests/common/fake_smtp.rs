//! A tiny SMTP server for tests: answers every command with success (no TLS, no auth) and
//! records each accepted message.

use std::sync::{Arc, Mutex};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

/// One received message.
#[derive(Debug, Clone, Default)]
pub struct Received {
    pub from: String,
    pub rcpt: Vec<String>,
    pub data: String,
}

/// Messages a fake server received.
pub type Inbox = Arc<Mutex<Vec<Received>>>;

/// Start a server on 127.0.0.1; its port and inbox.
pub async fn start() -> (u16, Inbox) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let inbox = Inbox::default();
    let log = inbox.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let log = log.clone();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut lines = BufReader::new(read).lines();
                let _ = write.write_all(b"220 fake ESMTP\r\n").await;
                let mut msg = Received::default();
                while let Ok(Some(line)) = lines.next_line().await {
                    let upper = line.to_ascii_uppercase();
                    let reply: &[u8] = if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                        b"250 fake\r\n"
                    } else if upper.starts_with("MAIL FROM:") {
                        msg.from = line[10..].trim().to_owned();
                        b"250 OK\r\n"
                    } else if upper.starts_with("RCPT TO:") {
                        msg.rcpt.push(line[8..].trim().to_owned());
                        b"250 OK\r\n"
                    } else if upper == "DATA" {
                        let _ = write.write_all(b"354 go\r\n").await;
                        while let Ok(Some(l)) = lines.next_line().await {
                            if l == "." {
                                break;
                            }
                            msg.data.push_str(&l);
                            msg.data.push('\n');
                        }
                        log.lock().unwrap().push(std::mem::take(&mut msg));
                        b"250 OK queued as FAKE123\r\n"
                    } else if upper == "QUIT" {
                        let _ = write.write_all(b"221 bye\r\n").await;
                        break;
                    } else {
                        b"250 OK\r\n"
                    };
                    let _ = write.write_all(reply).await;
                }
            });
        }
    });
    (port, inbox)
}
