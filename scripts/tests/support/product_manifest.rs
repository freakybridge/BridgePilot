//! Fixture access to the project partition of the unified factory manifest.
use serde_json::{Value, json};
use std::{fs, io, path::Path};

pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let value: Value = serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
    let mut bytes = serde_json::to_vec_pretty(&value["project_skeleton"]).map_err(io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn read_shared(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let value: Value = serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
    serde_json::to_vec_pretty(&value["shared_skills"]).map_err(io::Error::other)
}

pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(io::Error::other)
}

pub fn install(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.as_ref().parent().unwrap()).unwrap();
    write(path, bytes).unwrap();
}

pub fn write_shared(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    write_section(path, "shared_skills", bytes)
}

pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    write_section(path, "project_skeleton", bytes)
}

pub fn write_section(path: impl AsRef<Path>, name: &str, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().ok_or_else(|| io::Error::other("fixture manifest parent missing"))?)?;
    let mut value = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).map_err(io::Error::other)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => json!({"schema_version":1,"user_instructions":{},"shared_skills":{},"project_skeleton":{}}),
        Err(error) => return Err(error),
    };
    value[name] = serde_json::from_slice(bytes.as_ref()).map_err(io::Error::other)?;
    fs::write(path, serde_json::to_vec_pretty(&value).map_err(io::Error::other)?)
}
