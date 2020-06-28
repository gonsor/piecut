use std::{fmt, fs, time, io, io::Write, path, error::Error};
use clap::{Arg, ArgMatches, App};
use walkdir::WalkDir;
use piechart::{Chart, Color, Data};

const SECONDS_PER_DAY: u64 = 86400;

const NUM_FILES_SHOWN: usize = 5;

const SIZE_CONVERT_VALUE: f64 = 1024.;
const SIZE_CONVERT_SUFFIXES: [&str; 5] = ["Byte", "KiB", "MiB", "GiB", "TiB"];

struct LameFile {
    size: u64,
    path: path::PathBuf,
}

impl fmt::Display for LameFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:>11} -- {:?}", to_readable_size(self.size),
            self.path.file_name().unwrap())
    }
}

fn to_readable_size(size: u64) -> String {
    let base = (size.max(1) as f64).log(SIZE_CONVERT_VALUE);
    let floored = base.floor();
    let result = SIZE_CONVERT_VALUE.powf(base - floored);
    format!("{:.2} {}", result, SIZE_CONVERT_SUFFIXES[floored as usize])
}

fn parse_args<'a>() -> ArgMatches<'a> {
    App::new("Piecut")
        .version("1.0")
        .author("Daniel May")
        .about("Find files sorted by size and clean them up")
        .arg(Arg::with_name("DIR")
            .help("Directory that contains the input files")
            .required(true)
        ).arg(Arg::with_name("created")
            .short("c")
            .long("min-created")
            .value_name("DAYS")
            .help("Creation date must be at least DAYS in the past")
            .takes_value(true)
        ).arg(Arg::with_name("modified")
            .short("m")
            .long("min-modified")
            .value_name("DAYS")
            .help("Last modification date must be at least DAYS in the past")
            .takes_value(true)
        ).arg(Arg::with_name("accessed")
            .short("a")
            .long("min-accessed")
            .value_name("DAYS")
            .help("Last access date must be at least DAYS in the past")
            .takes_value(true)
        )
        .get_matches()
}

fn meets_time_condition(now: time::SystemTime, min_value: u64,
        actual_value: time::SystemTime) -> bool {
    if min_value == 0 {
        return true
    }
    if let Ok(dur) = now.duration_since(actual_value) {
        if dur.as_secs() > min_value {
            return true
        }
    }
    false
}

fn get_lame_files(path: &str, min_created: u64, min_modified: u64, min_accessed: u64)
        -> Result<(u64, Vec<LameFile>), Box<dyn Error>> {

    let now = time::SystemTime::now();
    let mut files = Vec::<LameFile>::new();
    let mut total_size: u64 = 0;

    for entry in WalkDir::new(path).into_iter() {
        match entry {
            Ok(file) => {
                let metadata = file.metadata()?;
                let size = file.metadata()?.len();
                if meets_time_condition(now, min_created, metadata.created()?)
                        && meets_time_condition(now, min_modified, metadata.modified()?)
                        && meets_time_condition(now, min_accessed, metadata.accessed()?) {
                    files.push(LameFile {
                        size,
                        path: file.path().to_path_buf()
                    });
                }
                total_size += size;
            },
            Err(error) => println!("{}. Skipping...", error)
        };
    }

    // sort by size, highest on top
    files.sort_by(|a, b| b.size.cmp(&a.size));

    Ok((total_size, files))
}

fn confirm_file_deletion(path: &path::Path) -> Result<bool, Box<dyn Error>>{
    let mut choice = String::new();
    print!("Delete file {}? y/N: ", path.to_str().unwrap());
    io::stdout().flush()?;
    io::stdin().read_line(&mut choice)?;
    if choice.trim().to_uppercase() == "Y" {
        fs::remove_file(path)?;
        println!("File deleted\n");
        return Ok(true)
    }
    Ok(false)
}

fn create_current_data(files: &Vec<LameFile>, skip: usize,
        total_size: u64, deleted: &[bool; NUM_FILES_SHOWN]) -> Vec<Data> {    

    let mut data_size: u64 = 0;
    let mut data = Vec::<Data>::new();
    
    // create data points for top files
    for (i, file) in files.iter()
                        .skip(skip)
                        .take(NUM_FILES_SHOWN)
                        .zip(deleted.iter())
                        .enumerate()
                        .filter(|(_, (_, &d))| !d)
                        .map(|(i, (f, _))| (i, f)) {           
        data.push(Data {
            label: format!("({}) {}", i + 1, file),
            value: file.size as f32 / total_size as f32,
            color: Some(Color::Fixed(i as u8 + 1)),
            fill: '•' 
        });
        data_size += file.size;
    }
    // show size of all other files as one datapoint
    let other_size = total_size - data_size;
    data.push(Data {
        label: format!("Other: {}", to_readable_size(other_size)),
        value: other_size as f32 / total_size as f32,
        color: Some(Color::RGB(100, 100, 100)),
        fill: '-' 
    } );

    data
}

fn parse_time_condition(matches: &ArgMatches, name: &str)
        -> Result<u64, Box<dyn Error>> {
    let val = matches.value_of(name).unwrap_or("0").parse::<u64>()?;
    if val > 0 {
        println!("Only showing files that were {} at least {} days ago.",
            name, val);
    }
    Ok(val)
}

fn process_input(files: &Vec<LameFile>, skip: &mut usize, total_size: &mut u64,
        deleted: &mut [bool; NUM_FILES_SHOWN]) -> Result<bool, Box<dyn Error>> {

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let choice = choice.trim().to_uppercase();
    
    match choice.as_str() {
        "N" => {
            *skip += NUM_FILES_SHOWN;
            *deleted = [false; NUM_FILES_SHOWN];
        },
        "Q" => return Ok(true),
        _ => {
            // get file to delete
            match choice.parse::<usize>() {
                Ok(n @ 1..=NUM_FILES_SHOWN) => {
                    let index = n - 1;
                    let file = &files[*skip + index];
                    // prompt for actual deletion
                    match confirm_file_deletion(&file.path) {
                        Ok(done) => {
                            if done {
                                deleted[index] = true;
                                *total_size -= file.size;
                            }
                        },
                        Err(_err) => eprintln!("Couldn't delete file")
                    };
                },
                // wrong number
                Ok(_) => eprintln!("Invalid choice"),
                Err(_err) => eprintln!("Not a valid number")
            };
        }
    };
    Ok(false)
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = parse_args();
    let path = matches.value_of("DIR").unwrap();

    println!("\nSearching for files in {} ...\n", path);

    let (mut total_size, files) = get_lame_files(path,
        parse_time_condition(&matches, "created")? * SECONDS_PER_DAY,
        parse_time_condition(&matches, "modified")? * SECONDS_PER_DAY,
        parse_time_condition(&matches, "accessed")? * SECONDS_PER_DAY
    )?;

    println!("\nTotal size: {}\n", to_readable_size(total_size));

    let mut skip: usize = 0;
    let mut deleted = [false; NUM_FILES_SHOWN];
    let mut quit = false;

    while !quit {
        let data = create_current_data(&files, skip, total_size, &deleted);

        Chart::new()
            .radius(6)
            .aspect_ratio(3)
            .legend(true)
            .draw(&data);

        println!("\nTop {0} filesizes are shown above. Enter a number to delete, \
            type n to show the next {0} files or q to quit.", NUM_FILES_SHOWN);
        print!("Input: ");
        io::stdout().flush()?;

        quit = process_input(&files, &mut skip, &mut total_size, &mut deleted)?;
    }
    Ok(())
}