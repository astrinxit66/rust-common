use log::{
    self, info,
    Record, Level, LevelFilter,
    Metadata, SetLoggerError,
};
use async_std::{
    prelude::*,
    io::BufWriter, task,
    fs::{File, OpenOptions},
    sync::{Arc, RwLock},
};
use std::{
    fs,
    ffi::OsStr,
    sync::{RwLock as SyncRwLock},
    path::{Path, PathBuf},
    io::{Write, stdout, Stdout},
};
use serde::Deserialize;
use bytes::Bytes;
use crossbeam_channel::{unbounded, Sender};
use chrono::{Date, Utc};

const LOG_CX: &str = "logit";
const DEF_DTIME_FMT: &str = "%Y/%m/%d %H:%M:%S";
const DEF_VERBOSITY: LevelFilter = LevelFilter::Trace;
const DEF_WBUF_SIZE: usize = 8192; //8KiB
const FILE_DATE_FMT: &str = "%Y_%m_%d";

type TripleTgtLvlMsg = (String, String, String);

pub trait Appender: Sync + Send {
    fn cx(&self) -> &Vec<String>;
    fn delegate(&self, rec: &Record);
    fn init(&mut self) {}
    fn flush(&self) {}
}

pub fn init<P>(cfg: P) -> Result<LogHandle, SetLoggerError> 
where P: AsRef<Path> {
    let cfg = cfg.as_ref();
    let log_cfg: LogitCfg = super::cfg::from_toml_path(super::path::from_app_root(cfg.to_str().unwrap()))
        .expect(format!("Logger cfg file {} exists and be readable", cfg.display()).as_str());
    let verbosity = log_cfg.verbosity;

    let file_apdrs = if log_cfg.file_apdrs.is_some() {
        let apdrs = log_cfg.file_apdrs.unwrap();
        apdrs.iter()
            .map(|file_apdr| Box::new(FileAppender::new(file_apdr)) as Box<dyn Appender>)
            .collect()
    } else { vec![] };

    let term_apdrs = if log_cfg.term_apdrs.is_some() {
        let apdrs = log_cfg.term_apdrs.unwrap();
        apdrs.iter()
            .map(|term_apdr| Box::new(TermAppender::new(term_apdr, false)) as Box<dyn Appender>)
            .collect()
    } else { vec![] };

    let appenders: Arc<SyncRwLock<Vec<Box<dyn Appender>>>> = Arc::new(SyncRwLock::new(Vec::new()));
    
    Ok(task::block_on(async {
        let mut lock = appenders.write().unwrap();
        lock.extend(file_apdrs);
        lock.extend(term_apdrs);
        lock.iter_mut().for_each(|apdr: &mut Box<dyn Appender>| apdr.init());

        let logger = {
            let mut cx: Vec<String> = vec![];
            let default_appender = TermAppender::new(&TermAppenderCfg {
                sources: vec![],
                verbosity: Some(DEF_VERBOSITY)
            }, true);

            lock.iter().for_each(|apdr| cx.extend(apdr.cx().iter().cloned()));
            Logit {cx, default_appender, appenders: appenders.clone()}
        };

        std::mem::drop(lock);

        let handle = LogHandle(appenders);

        log::set_boxed_logger(Box::new(logger)).map(|()| log::set_max_level(verbosity))
            .expect("should init Logit for the first time");

        handle
    }))
}

pub struct LogHandle(Arc<SyncRwLock<Vec<Box<dyn Appender>>>>);

impl Drop for LogHandle {
    fn drop(&mut self) {
println!(">>> dropping LogHandle, flush all appenders");
        let apdrs = self.0.read().unwrap();
        apdrs.iter().for_each(|apdr| apdr.flush());
    }
}

struct Logit {
    cx: Vec<String>,
    default_appender: TermAppender,
    appenders: Arc<SyncRwLock<Vec<Box<dyn Appender>>>>
}

#[derive(Deserialize)]
struct LogitCfg {
    verbosity: LevelFilter,
    file_apdrs: Option<Vec<FileAppenderCfg>>,
    term_apdrs: Option<Vec<TermAppenderCfg>>,
}

#[derive(Deserialize)]
struct FileAppenderCfg {
    sources: Vec<String>,
    target: PathBuf,
    time_format: Option<String>,
    bytes_rotation_size: Option<u64>,
    verbosity: Option<LevelFilter>,
}

#[derive(Deserialize)]
struct TermAppenderCfg {
    sources: Vec<String>,
    verbosity: Option<LevelFilter>
}

struct AsyncFileAppender {
    buf: RwLock<BufWriter<File>>,
    time_format: String,
    path: PathBuf,
    filename_mask: (String, String),
    bytes_rotation_size: Option<u64>,
    bytes_current_size: u64,
    today: Date<Utc>
}

struct FileAppender {
    cx: Vec<String>,
    tx: Option<Sender<TripleTgtLvlMsg>>,
    verbosity: LevelFilter,
    apdr: Arc<RwLock<AsyncFileAppender>>
}

struct TermAppender {
    cx: Vec<String>,
    verbosity: LevelFilter,
    out: Stdout
}


impl log::Log for Logit {
    fn enabled(&self, meta: &Metadata) -> bool {
        // prevent log of async_std::task::builder bc Logit itself uses tasks
        meta.target() != "async_std::task::builder"
    }

    fn log(&self, rec: &Record) {
        if rec.target() == LOG_CX || !self.delegable(rec.metadata()) {
            self.default_appender.delegate(rec);
        } else {
            task::block_on(async {
                let apdrs = self.appenders.read().unwrap();
                apdrs.iter().for_each(|apdr| apdr.delegate(rec));
            })
        }
    }

    fn flush(&self) {}
}

impl Logit {
    fn delegable(&self, meta: &Metadata) -> bool {
        for cx in self.cx.iter() {
            if &cx[..] == meta.target() {
                return true;
            }
        }

        false
    }

    fn should_write(cx: &Vec<String>, target: &str, verbosity: &LevelFilter, rec_level: &Level) -> bool {
        if verbosity == &LevelFilter::Off { return false; }
        if rec_level > verbosity { return false; }
        if cx.is_empty() { return true; }
        cx.iter().filter(|x| &x[..] == target).count() > 0
    }

    fn fmt(target: &str, verbosity: &str, msg: &str, custom_time_fmt: Option<&str>) -> Bytes {
        let now = Utc::now();
        let time_fmt = custom_time_fmt.unwrap_or(DEF_DTIME_FMT);

        Bytes::from(format!("[{}] {:10} [{}] {}\n",
            now.format(time_fmt),
            target.to_uppercase(),
            verbosity,
            msg
        ))
    }
}

impl Appender for TermAppender {
    fn cx(&self) -> &Vec<String> {
        &self.cx
    }

    fn delegate(&self, rec: &Record) {
        if Logit::should_write(&self.cx, rec.target(), &self.verbosity, &rec.level()) {
            let mut out = self.out.lock();

            out.write_all(&Logit::fmt(
                &format!("{}", rec.target()),
                &format!("{}", rec.level()),
                &format!("{}", *rec.args()),
                None
            ))
            .expect("write to TermAppender's");
        }
    }
}

impl TermAppender {
    fn new(cfg: &TermAppenderCfg, is_default: bool) -> Self {
        if !is_default {
            assert!(!cfg.sources.is_empty(), "terminal logger sources cannot be empty");
        }

        let verbosity = cfg.verbosity.unwrap_or(DEF_VERBOSITY);

        TermAppender {
            cx: cfg.sources.to_owned(),
            verbosity,
            out: stdout()
        }
    }
}

impl Appender for FileAppender {
    fn cx(&self) -> &Vec<String> {
        &self.cx
    }

    fn init(&mut self) {
        let (tx, rx) = unbounded::<TripleTgtLvlMsg>();
        let apdr = self.apdr.clone();

        self.tx = Some(tx);

        task::spawn(async move {
            let mut apdr = apdr.write().await;

            loop {
                if let Ok(log) =rx.recv() {
                    if log == FileAppender::SIGKILL {
                        std::mem::drop(apdr);
                        break;
                    } else {
                        apdr.append(log).await;
                    }
                }
            }
        });
    }

    fn delegate(&self, rec: &Record) {
        if Logit::should_write(&self.cx, rec.target(), &self.verbosity, &rec.level()) {
            if let Some(tx) = &self.tx {
                tx.send((
                    format!("{}", rec.target()),
                    format!("{}", rec.level()),
                    format!("{}", *rec.args()),
                ))
                .expect("transmit log entry to FileAppender's writer");
            }
        }
    }

    fn flush(&self) {
        task::block_on(async {
            if let Some(tx) = &self.tx {
                tx.send(FileAppender::SIGKILL).expect("should send SIGKILL");
                
                let apdr = self.apdr.write().await;
                let mut buf = apdr.buf.write().await;

                buf.flush().await.expect("flush FileAppender's inner buffer");
            }
        });
    }
}

impl FileAppender {
    const SIGKILL: TripleTgtLvlMsg = (String::new(), String::new(), String::new());

    fn new(cfg: &FileAppenderCfg) -> Self {
        let verbosity = cfg.verbosity.unwrap_or(DEF_VERBOSITY);

        task::block_on(async {
            FileAppender {
                cx: cfg.sources.clone(),
                tx: None,
                verbosity,
                apdr: Arc::new(RwLock::new(AsyncFileAppender::init(cfg)))
            }
        })
    }
}

impl AsyncFileAppender {
    fn init(cfg: &FileAppenderCfg) -> Self {
        let mut path_buf = cfg.target.clone();
        let time_format = cfg.time_format.to_owned().unwrap_or(String::from(DEF_DTIME_FMT));
        let bytes_rotation_size = cfg.bytes_rotation_size;
        let filename_mask = (
            path_buf.file_stem().unwrap().to_str().unwrap().to_owned(),
            path_buf.extension().map(|x| x.to_str().unwrap().to_owned()).unwrap_or(String::new())
        );
        let (daily_based_filename, today) = AsyncFileAppender::dated_filename(&filename_mask, None);

        if path_buf.is_relative() {
            if path_buf.starts_with("./") {
                path_buf = PathBuf::from(path_buf.strip_prefix("./").unwrap());
            }

            path_buf = super::path::from_app_root(path_buf.to_str().unwrap());
        }

        path_buf.set_file_name(OsStr::new(&daily_based_filename));

        if let Some(dir_path) = path_buf.parent() {
            fs::create_dir_all(dir_path)
                .expect(&format!("should have been able to create missing folders of {}", &path_buf.display()));
        }

        task::block_on(async {
            AsyncFileAppender {
                buf: AsyncFileAppender::new_buf(&path_buf).await,
                path: path_buf.clone(),
                filename_mask,
                time_format,
                bytes_rotation_size,
                bytes_current_size: AsyncFileAppender::file_size(&path_buf).await,
                today
            }
        })
    }

    async fn write(&self, bytes: &[u8]) {
        let mut buf = self.buf.write().await;

        buf.write(bytes).await.expect("write log entry to buffer");

        if buf.buffer().len() >= DEF_WBUF_SIZE {
            buf.flush().await.expect("write log entry down to file");
        }
    }

    async fn new_buf(path: &Path) -> RwLock<BufWriter<File>> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path).await
            .expect(&format!("folder {} should exist and be writable", path.parent().unwrap().display()));
        
            RwLock::new(BufWriter::new(file))
    }

    async fn file_size(path: &Path) -> u64 {
        OpenOptions::new()
            .read(true)
            .open(path).await
            .expect(format!("{} should exist and be readable", path.to_str().unwrap()).as_str())
            .metadata().await.unwrap()
            .len()
    }

    fn dated_filename(mask: &(String, String), sfx: Option<&str>) -> (String, Date<Utc>) {
        let today = Utc::today();

        (format!("{}_{}{}.{}",
            &mask.0,
            today.format(FILE_DATE_FMT),
            sfx.map(|s| String::from("-") + s).unwrap_or(String::new()),
            &mask.1
        ), today)
    }

    async fn rotate(&mut self, path: &PathBuf, is_daily: bool) {
        info!(target: LOG_CX, "Rotate cause {}, new log file {}",
            if is_daily { String::from("daily change") } else {
                format!("size limit of {}B reached (current is {}B)",
                    self.bytes_rotation_size.unwrap(),
                    self.bytes_current_size
                )
            },
            path.to_str().unwrap()
        );

        self.bytes_current_size = 0;
        self.buf = AsyncFileAppender::new_buf(path).await;
        self.path = path.to_owned();
    }

    async fn daily_rotate(&mut self) {
        let date = Utc::today();
        let past_days = date.signed_duration_since(self.today);

        if past_days.num_days() > 0 {
            let mut path_buf = self.path.clone();
            let (new_filename, today) = AsyncFileAppender::dated_filename(&self.filename_mask, None);

            path_buf.set_file_name(OsStr::new(new_filename.as_str()));

            self.today = today;
            self.rotate(&path_buf, true).await;
        } 
    }

    async fn auto_rotate(&mut self) {
        self.daily_rotate().await;

        if let Some(rotation_size) = self.bytes_rotation_size {
            if self.bytes_current_size >= rotation_size {
                let mut buf = self.buf.write().await;
                buf.flush().await.expect("flush AsyncFileAppender's buffer");
                std::mem::drop(buf);

                let mut path_buf = self.path.clone();
                let mut i = 1u16;

                while {
                    let (new_filename, _) = AsyncFileAppender::dated_filename(&self.filename_mask, Some(&i.to_string()));
                    path_buf.set_file_name(OsStr::new(&new_filename));
                    i += 1;

                    path_buf.exists()
                }{}

                self.rotate(&path_buf, false).await;
            }
        }
    }

    async fn append(&mut self, log: TripleTgtLvlMsg) {
        self.auto_rotate().await;

        let log_entry = Logit::fmt(&log.0, &log.1, &log.2, Some(&self.time_format));
        self.bytes_current_size = self.bytes_current_size + log_entry.len() as u64;

        self.write(&log_entry).await;
    }
} 