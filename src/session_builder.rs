use std::process::Stdio;

use tokio::{
    io::{BufReader, duplex, split},
    process::Command,
};

use crate::{Handler, Session, SessionError};

impl Session {
    pub fn new_channel(
        handler0: impl Handler + Send + Sync + 'static,
        handler1: impl Handler + Send + Sync + 'static,
    ) -> (Session, Session) {
        let (d0, d1) = duplex(1024);
        let (r0, w0) = split(d0);
        let (r1, w1) = split(d1);
        let s0 = Session::new(handler0, BufReader::new(r0), w0);
        let s1 = Session::new(handler1, BufReader::new(r1), w1);
        (s0, s1)
    }

    pub fn from_stdio(handler: impl Handler + Send + Sync + 'static) -> Session {
        Session::new(
            handler,
            BufReader::new(tokio::io::stdin()),
            tokio::io::stdout(),
        )
    }

    pub fn from_command(
        handler: impl Handler + Send + Sync + 'static,
        command: &mut Command,
    ) -> Result<Session, SessionError> {
        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.unwrap();
        let stdout = child.stdout.unwrap();
        Ok(Session::new(handler, BufReader::new(stdout), stdin))
    }
}
