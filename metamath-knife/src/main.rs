//! A library for manipulating [Metamath](http://us.metamath.org/#faq)
//! databases.  The entry point for all API operations is in the `database`
//! module, as is a discussion of the data representation.

use std::{fs::{self, File}, io::Read};

use clap::{clap_app, crate_version};
use metamath_rs::database::{Database, DbOptions};

fn main() {
    let app = clap_app!(("smetamath-knife") =>
        (version: crate_version!())
        (about: "A Metamath database verifier and processing tool")
        (@arg DATABASE: required(true) "Database file to load")
    );

    let matches = app.get_matches();

    let options = DbOptions::default();

    let mut db = Database::new(options);

    let mut data = Vec::new();
    let start = matches
        .value_of("DATABASE")
        .map(|x| x.to_owned()).unwrap();
    let metadata = fs::metadata(&start).unwrap();
    let mut fh = File::open(&start).unwrap();
    let mut buf = Vec::with_capacity(metadata.len() as usize + 1);
    // note: File's read_to_end uses the buffer capacity to choose how much to read
    fh.read_to_end(&mut buf).unwrap();
    data.push((start.clone(), buf));

    // Parsing
    db.parse_and_name_scope_passes(start, data);
    let parse_result = db.parse_result().as_ref().clone();
    let name_result = db.name_result().as_ref().clone();
    let scope_result = db.scope_result().as_ref().clone();

/* Serializing/deserializing the data in memory

    let parse_serialized = ron::to_string(&parse_result).unwrap();
    let name_serialized = ron::to_string(&name_result).unwrap();
    let scope_serialized = ron::to_string(&scope_result).unwrap();

    let parse_result = ron::from_str(&parse_serialized).unwrap();
    let name_result = ron::from_str(&name_serialized).unwrap();
    let scope_result = ron::from_str(&scope_serialized).unwrap();
*/

    // Init new database with parsed data
    let mut db_new = Database::new(options);
    db_new.init_verify(parse_result, name_result, scope_result);

    // Verify
    let count_diags = db_new.verify();

    println!("{count_diags} diagnostics issued.");

    if count_diags > 0 { std::process::exit(1); }

}
