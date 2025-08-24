include!("./src/cnf/schema.rs");

use std::ffi::OsString;
use std::fs::{self};
use std::path::Path;

pub fn generate_schema(outdir: &OsString) {
    let schema = schemars::schema_for!(Config);
    let schema_file = Path::new(outdir).join("schema.json");

    fs::write(schema_file, serde_json::to_string_pretty(&schema).unwrap()).unwrap();
}

fn main() {
    let outdir = OsString::from("./schemas");

    fs::create_dir_all(&outdir).unwrap();

    generate_schema(&outdir);
}
