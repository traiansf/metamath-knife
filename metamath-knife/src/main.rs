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


    let count_diags = db.parse_and_verify(start.clone(), data.clone());

    println!("{count_diags} diagnostics issued.");

    if count_diags > 0 { std::process::exit(1); }

}
