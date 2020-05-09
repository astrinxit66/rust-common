extern crate astrinxit;

use astrinxit::logit;
use log::{trace, info, debug, warn, error};

const EXAMPLE: &str = "example";
const SOURCE1: &str = "source1";
const SOURCE2: &str = "source2";
const SOURCE3: &str = "source3";

fn main() {
    let log_handle = logit::init("examples/config/logit.toml").expect("init logit as log implementation");

    trace!("should be a noop");
    trace!(target: EXAMPLE,"should be a noop");
    trace!(target: SOURCE1,"should be a noop");
    trace!(target: SOURCE2,"should be a noop");
    

    debug!("should be displayed on the terminal which is the default Logit appender");
    info!("should be displayed on the terminal which is the default Logit appender");
    warn!("should be displayed on the terminal which is the default Logit appender");
    error!("should be displayed on the terminal which is the default Logit appender");

    debug!(target: EXAMPLE, "should be a noop");
    info!(target: EXAMPLE, "should be written into example_info.log and displayed into terminal");
    warn!(target: EXAMPLE, "should be written into example_info.log and displayed into terminal");
    error!(target: EXAMPLE, "should be written into example_info.log, example_error.log and displayed into terminal");
    error!(target: EXAMPLE, "should have %Y/%m/%d %H:%M:%S time format on terminal and on example_info.log, but C-like time format on example_error.log");

    debug!(target: SOURCE1, "should be a noop");
    info!(target: SOURCE1, "should be displayed into terminal");
    warn!(target: SOURCE1, "should be displayed into terminal");
    error!(target: SOURCE1, "should be displayed into terminal");

    debug!(target: SOURCE2, "should be a noop");
    info!(target: SOURCE2, "should be written into example_info.log");
    warn!(target: SOURCE2, "should be written into example_info.log");
    error!(target: SOURCE2, "should be written into example_info.log");

    debug!(target: SOURCE3, "should be a noop");
    info!(target: SOURCE3, "should be a noop");
    warn!(target: SOURCE3, "should be displayed into terminal");
    error!(target: SOURCE3, "should be displayed into terminal and written into example_error.log");
}