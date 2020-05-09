pub mod logit;
pub mod cfg;
pub mod codec;

pub use log;

pub mod path {
    use std::{
        path::PathBuf,
        env
    };

    pub fn from_app_root(rel_path: &str) -> PathBuf {
        let mut cwd = env::current_dir().expect("get current working directory");

        cwd.push(rel_path);
        cwd
    }
}