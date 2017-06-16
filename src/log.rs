// Copyright © 2017 Felix Obenhuber
// This program is free software. It comes without any warranty, to the extent
// permitted by applicable law. You can redistribute it and/or modify it under
// the terms of the Do What The Fuck You Want To Public License, Version 2, as
// published by Sam Hocevar. See the COPYING file for more details.

use clap::ArgMatches;
use errors::*;
use futures::future::*;
use futures::{Future, Async, AsyncSink, Stream, Sink, Poll, StartSend};
use std::process::{Command, Stdio};
use super::Message;
use super::adb;
use super::reader::StdinReader;
use super::record::Level;
use tokio_core::reactor::{Core, Handle};
use tokio_process::CommandExt;
use tokio_signal::ctrl_c;

struct Logger {
    handle: Handle,
    tag: String,
    level: Level,
}

impl Logger {
    fn level(level: &Level) -> &str {
        match level {
            &Level::Trace | &Level::Verbose => "v",
            &Level::Debug | &Level::None => "d",
            &Level::Info => "i",
            &Level::Warn => "w",
            &Level::Error | &Level::Fatal | &Level::Assert => "e",
        }
    }
}

impl Sink for Logger {
    type SinkItem = Message;
    type SinkError = Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if let Message::Record(r) = item {
            let child = Command::new(adb()?)
                .arg("shell")
                .arg("log")
                .arg("-p")
                .arg(Self::level(&self.level))
                .arg("-t")
                .arg(format!("\"{}\"", &self.tag))
                .arg(&r.raw)
                .stdout(Stdio::piped())
                .output_async(&self.handle)
                .map(|_| ())
                .map_err(|_| ());
            self.handle.spawn(child);
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(Async::Ready(()))
    }
}

pub fn run(args: &ArgMatches, core: &mut Core) -> Result<i32> {
    let message = args.value_of("MESSAGE").unwrap_or("");
    let tag = args.value_of("TAG").unwrap_or("Rogcat").to_owned();
    let level = Level::from(args.value_of("LEVEL").unwrap_or(""));
    match message {
        "-" => {
            let handle = core.handle();
            let ctrlc = core.run(ctrl_c(&handle))?
                .map(|_| Message::Done)
                .map_err(|e| e.into());
            let sink = Logger {
                handle: core.handle(),
                tag,
                level,
            };

            let input = StdinReader::new(core);
            let result = input.select(ctrlc).map(|m| m).take_while(|r| ok(r != &Message::Done));
            let stream = sink.send_all(result);
            core.run(stream).map_err(|_| "Failed to run \"adb shell log\"".into()).map(|_| 0)
        }
        _ => {
            let child = Command::new(adb()?)
                .arg("shell")
                .arg("log")
                .arg("-p")
                .arg(&Logger::level(&level))
                .arg("-t")
                .arg(&tag)
                .arg(format!("\"{}\"", message))
                .stdout(Stdio::piped())
                .output_async(&core.handle())
                .map(|_| ())
                .map_err(|_| ());
            core.run(child).map_err(|_| "Failed to run \"adb shell log\"".into()).map(|_| 0)
        }
    }
}
