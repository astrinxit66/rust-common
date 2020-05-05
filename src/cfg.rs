use async_std::io;

pub use serdeconv::from_toml_file as from_toml_path;

pub mod certs {
    use super::*;
    use std::{
        io::BufReader,
        path::Path,
        fs::File
    };
    use rustls::{
        Certificate, PrivateKey, ServerConfig, NoClientAuth,
        internal::pemfile::{certs, rsa_private_keys}
    };

    pub fn from_file_path<P>(path: P) -> io::Result<Vec<Certificate>> 
    where P: AsRef<Path> {
        certs(&mut BufReader::new(File::open(path)?))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
    }

    pub fn pk_from_path<P>(path: P) -> io::Result<Vec<PrivateKey>>
    where P: AsRef<Path> {
        rsa_private_keys(&mut BufReader::new(File::open(path)?))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))
    }

    pub fn tls_cfg<P>(cert_file: P, pk_file: P) -> io::Result<ServerConfig> 
    where P: AsRef<Path> {
        let certs = certs::from_file_path(cert_file)?;
        let mut pkeys = certs::pk_from_path(pk_file)?;

        let mut cfg = ServerConfig::new(NoClientAuth::new());

        cfg
            .set_single_cert(certs, pkeys.remove(0))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        
        Ok(cfg)
    }
}