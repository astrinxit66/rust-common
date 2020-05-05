use log::{
    self, info,
    Record, Level, LevelFilter,
    Metadata, SetLoggerError
};
use async_std::io;
use std::{
    path::Path,
    io::{stdout, Stdout}
};

// TODO TOContinued

pub trait Appender: Sync + Send {
    fn cx(&self) -> &Vec<String>;
    fn delegate(&self, rec: &Record);
    fn init(&mut self) {}
}

pub fn init<P>(cfg: P) -> Result<(), SetLoggerError> 
where P: AsRef<Path> {

    Ok(())
}

pub struct Logger {
    cx: Vec<String>,
    max_level: LevelFilter,
    default_appender: TermAppender,
    appenders: Vec<Box<dyn Appender>>
}

struct TermAppender {
    cx: Vec<String>,
    level: LevelFilter,
    out: Stdout
}