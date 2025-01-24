use chrono::{DateTime, Utc};
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use md5::{self, Digest};
use std::io::{self, Read};
use glob;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use std::{collections::HashMap, time::Duration};
use std::{fs, thread};

static DEBUG_DELAY: u64 = 0;

#[derive(Parser, Debug)]
#[command(name = "Dupefindr", version)]
#[command(about = "A tool to find duplicate files", long_about = None)]
#[command(propagate_version = true)]
struct Args {
    /// The directory to search for duplicates in.
    #[arg(short, long, default_value = ".")]
    path: String,

    /// wildcard pattern to search for
    /// Example: *.txt
    #[arg(short, long, default_value = "*")]
    wildcard: String,

    /// Recursively search for duplicates
    #[arg(short, long)]
    recursive: bool,

    /// Display debug information
    #[arg(short, long, default_value = "true")]
    debug: bool,

    /// Include 0 byte files
    #[arg(long, default_value = "false")]
    include_zero_byte_files: bool,

    /// Dry run the program
    /// This will not delete or modify any files
    #[arg(long, default_value = "false")]
    dry_run: bool,

    /// Include hidden files
    #[arg(long, default_value = "false")]
    include_hidden_files: bool,

    /// Display verbose output
    #[arg(long, default_value = "false")]
    verbose: bool,
}

#[derive(Debug)]
struct FileInfo {
    path: String,
    size: u64,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
}

fn main() {
    let args = get_command_line_arguments();

    match start_search(&args) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn get_command_line_arguments() -> Args {
    let args = Args::parse();
    if args.debug {
        println!("Searching for duplicates in: {}", args.path);
        if args.recursive {
            println!("Recursively searching for duplicates");
        }
        println!("Include empty files: {}", args.include_zero_byte_files);
        println!("Dry run: {}", args.dry_run);
        println!("Include hidden files: {}", args.include_hidden_files);
        println!("Verbose: {}", args.verbose);
        println!("Wildcard: {}", args.wildcard);
        println!();
    }
    args
}
fn start_search(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let multi = MultiProgress::new();
    // get the files in the directory
    let folder_path: String = args.path.clone();
    let mut hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();
    let _result = get_files_in_directory(args, folder_path, &multi);
    let _files = match _result {
        Ok(files) => files,
        Err(e) => {
            println!("Error: {}", e);
            return Err(e);
        }
    };
    if args.verbose {
        println!("Found {} files", _files.len());
    }
    //bar.finish_and_clear();
    let sty_dupes =
        ProgressStyle::with_template("ETA {eta} {bar:40.yellow/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-");
    let sty_processing = ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap()
        .progress_chars("##-");
    let bar = multi.add(ProgressBar::new(_files.len() as u64));
    bar.set_style(sty_dupes);
    let bar2 = multi.add(ProgressBar::new_spinner());
    bar2.enable_steady_tick(Duration::from_millis(100));
    bar2.set_style(sty_processing);
    bar2.set_message("Identifying duplicates...");
    for file in _files {
        let hash_string = get_hash_of_file(&file.path, &bar);
        if args.verbose {
            bar.println(format!(
                "File: {} [{} bytes] [hash: {}]",
                file.path, file.size, hash_string
            ));
        }
        if !hash_map.contains_key(&hash_string) {
            let mut vec = Vec::new();
            vec.push(file);
            hash_map.insert(hash_string, vec);
        } else {
            let vec = hash_map.get_mut(&hash_string).unwrap();
            vec.push(file);
        }
        bar.inc(1);
        if args.debug {
            thread::sleep(Duration::from_millis(DEBUG_DELAY));
        }
    }
    bar.finish_and_clear();
    bar2.finish_and_clear();
    // print the duplicates
    let mut duplicates_found = 0;
    for (hash, files) in hash_map.iter() {
        if files.len() > 1 {
            duplicates_found += 1;
            println!("Found {} duplicates for hash: {}", files.len(), hash);
            for file in files {
                println!(
                    "File: {} [created: {}] [modified: {}] [{} bytes]",
                    file.path,
                    file.created_at.to_rfc2822(),
                    file.modified_at.to_rfc2822(),
                    file.size
                );
            }
        }
    }
    if duplicates_found == 0 {
        println!("No duplicates found");
    } else {
        println!("Found {} duplicate instances", duplicates_found);
    }
    multi.remove(&bar2);
    multi.remove(&bar);
    multi.clear().unwrap();
    Ok(())
}

fn get_files_in_directory(
    args: &Args,
    folder_path: String,
    multi: &MultiProgress,
) -> Result<Vec<FileInfo>, Box<dyn std::error::Error>> {
    let mut files: Vec<FileInfo> = Vec::new();
    //let dir_path = std::path::Path::new(folder_path.as_str());
    match fs::metadata(folder_path.as_str()) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(Box::<dyn std::error::Error>::from(
                    "The path provided is not a directory",
                ));
            }
        }
        Err(_) => {
            return Err(Box::from("The path provided is not a directory"));
        }
    }
    let entries = fs::read_dir(&folder_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    let sty_folders = ProgressStyle::with_template("{bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
        .unwrap()
        .progress_chars("##-");

    let sty_files = ProgressStyle::with_template("{bar:40.green/green} {pos:>7}/{len:7} {msg}")
        .unwrap()
        .progress_chars("##-");

    let sty_processing = ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap()
        .progress_chars("##-");

    // process directories first
    let bar = multi.add(ProgressBar::new_spinner());
    bar.set_style(sty_processing);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message("Processing objects");
    let mut folder_count = 0;
    let mut file_count = 0;
    let mut folders: Vec<PathBuf> = Vec::new();
    for entry in entries.iter() {
        if entry.is_dir() {
            folder_count += 1;
            folders.push(entry.clone());
        } else {
            file_count += 1;
        }
    }
    bar.finish_and_clear();
    multi.remove(&bar);

    let bar2 = multi.add(ProgressBar::new(folder_count as u64));
    bar2.set_style(sty_folders);

    for fld in folders.iter() {
        bar2.set_message(format!("Folder {}", fld.to_str().unwrap().to_string()));
        let hidden: bool;
        #[cfg(not(target_os = "windows"))]
        {
            hidden = fld.file_name().unwrap().to_str().unwrap().starts_with(".");
        }
        #[cfg(target_os = "windows")]
        {
            if std::fs::metadata(&path)
                .unwrap()
                .file_attributes()
                .hidden()
                .unwrap()
            {
                hidden = true;
            }
        }

        if hidden {
            if args.include_hidden_files == false {
                if args.verbose {
                    let _ = multi.println(format!(
                        "Ignoring hidden directory: {}",
                        fld.file_name().unwrap().to_str().unwrap()
                    ));
                }
                bar2.inc(1);
                continue;
            }
        }

        if !args.recursive {
            if args.verbose {
                let _ = multi.println(format!(
                    "Ignoring directory: {}",
                    fld.file_name().unwrap().to_str().unwrap()
                ));
            }
        } else {
            let path = fld.as_path();

            let sub_files =
                get_files_in_directory(args, path.to_str().unwrap().to_string(), &multi)?;
            files.extend(sub_files);
        }
        bar2.inc(1);
    }
    bar2.finish_and_clear();
    multi.remove(&bar2);

    let bar2 = multi.add(ProgressBar::new(file_count as u64));
    bar2.set_style(sty_files);

    for entry in entries.iter() {
        let path = entry.as_path();
        let _ = bar2.set_message(format!("Processing: {}", path.to_str().unwrap()));

        if path.is_file() {

            // determine if the file matches the wildcard
            let wildcard_pattern = glob::Pattern::new(&args.wildcard)?;
            if !wildcard_pattern.matches_path(path) {
                if args.verbose {
                    let _ = multi.println(format!("Ignoring file (does not match wildcard): {}", path.to_str().unwrap()));
                }
                bar2.inc(1);
                continue;
            }
            
            let hidden: bool;
            #[cfg(not(target_os = "windows"))]
            {
                hidden = path.file_name().unwrap().to_str().unwrap().starts_with(".");
            }
            #[cfg(target_os = "windows")]
            {
                if std::fs::metadata(&path)
                    .unwrap()
                    .file_attributes()
                    .hidden()
                    .unwrap()
                {
                    hidden = true;
                }
            }
            if args.include_hidden_files == false && hidden {
                if args.verbose {
                    let _ =
                        multi.println(format!("Ignoring hidden file: {}", path.to_str().unwrap()));
                }
                if args.debug {
                    thread::sleep(Duration::from_millis(DEBUG_DELAY));
                }
                bar2.inc(1);
                continue;
            }
            let meta = std::fs::metadata(&path).unwrap();
            let size = meta.len();
            if size == 0 && !args.include_zero_byte_files {
                if args.verbose {
                    let _ =
                        multi.println(format!("Ignoring empty file: {}", path.to_str().unwrap()));
                }
                if args.debug {
                    thread::sleep(Duration::from_millis(DEBUG_DELAY));
                }
                bar2.inc(1);
                continue;
            }

            let created_at = meta.created().unwrap();
            let modified_at = meta.modified().unwrap();
            // Convert SystemTime to chrono::DateTime<Utc>
            let created_at_utc_datetime: DateTime<Utc> = DateTime::from(UNIX_EPOCH)
                + chrono::Duration::from_std(created_at.duration_since(UNIX_EPOCH).unwrap())
                    .unwrap();
            let modified_at_utc_datetime: DateTime<Utc> = DateTime::from(UNIX_EPOCH)
                + chrono::Duration::from_std(modified_at.duration_since(UNIX_EPOCH).unwrap())
                    .unwrap();

            let file_info = FileInfo {
                path: path.to_str().unwrap().to_string(),
                size,
                created_at: created_at_utc_datetime,
                modified_at: modified_at_utc_datetime,
            };
            files.push(file_info);

            if args.debug {
                thread::sleep(Duration::from_millis(DEBUG_DELAY));
            }
            bar2.inc(1);
        }
    }

    bar2.finish_and_clear();
    Ok(files)
}

fn get_hash_of_file(file_path: &str, _bar: &ProgressBar) -> String {
    let mut file = std::fs::File::open(file_path).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    return get_md5_hash(&buffer);
}

fn get_md5_hash(buffer: &Vec<u8>) -> String {
    let mut hasher = md5::Md5::new();
    hasher.update(&buffer);
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_get_command_line_arguments() {
        // test default values for cmd args
        let args = get_command_line_arguments();
        assert_eq!(args.path, ".");
        assert_eq!(args.recursive, false);
        assert_eq!(args.debug, true);
        assert_eq!(args.include_zero_byte_files, false);
        assert_eq!(args.dry_run, false);
        assert_eq!(args.include_hidden_files, false);
        assert_eq!(args.verbose, false);
        assert_eq!(args.wildcard, "*");
    }

    #[test]
    fn test_get_files_in_directory() {
        let multi = MultiProgress::new();
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let files = get_files_in_directory(&args, "data".to_string(), &multi).unwrap();
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn test_get_files_in_directory_wildcard() {
        let multi = MultiProgress::new();
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
            verbose: false,
            wildcard: "*testdupe*.txt".to_string(),
        };
        let files = get_files_in_directory(&args, "data".to_string(), &multi).unwrap();
        assert_eq!(files.len(), 4);
    }

    #[test]
    fn test_get_files_in_directory_include_empty() {
        let multi = MultiProgress::new();
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: true,
            dry_run: false,
            include_hidden_files: false,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let files = get_files_in_directory(&args, "data".to_string(), &multi).unwrap();
        assert_eq!(files.len(), 7);
    }

    #[test]
    fn test_get_files_in_directory_include_hidden() {
        let multi = MultiProgress::new();
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: true,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let files = get_files_in_directory(&args, "data".to_string(), &multi).unwrap();
        assert_eq!(files.len(), 6);
    }

    #[test]
    fn test_get_files_in_directory_include_all_files() {
        let multi = MultiProgress::new();
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: true,
            dry_run: false,
            include_hidden_files: true,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let files = get_files_in_directory(&args, "data".to_string(), &multi).unwrap();
        assert_eq!(files.len(), 8);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive() {
        let multi = MultiProgress::new();
        let args = Args {
            path: "data".to_string(),
            recursive: true,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let files = get_files_in_directory(&args, "data".to_string(), &multi).unwrap();
        assert_eq!(files.len(), 16);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive_with_hidden() {
        let multi = MultiProgress::new();
        let args = Args {
            path: "data".to_string(),
            recursive: true,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: true,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let files = get_files_in_directory(&args, "data".to_string(), &multi).unwrap();
        assert_eq!(files.len(), 18);
    }

    #[test]
    fn test_get_files_in_directory_bad_path() {
        let multi = MultiProgress::new();
        let args = Args {
            path: "badpath!!!".to_string(),
            recursive: true,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let result = get_files_in_directory(&args, "badpath!!!".to_string(), &multi);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_hash_of_file() {
        let hash = get_hash_of_file("data//testdupe1.txt", &ProgressBar::new_spinner());
        assert_eq!(hash, "8c91214730e59f67bd46d1855156e762");
    }

    #[test]
    #[should_panic]
    fn test_get_hash_of_file_bad_path() {
        let hash = get_hash_of_file("data//testdupe1-notfound.txt", &ProgressBar::new_spinner());
        assert_eq!(hash, "8c91214730e59f67bd46d1855156e762");
    }

    #[test]
    fn test_get_md5_hash() {
        let buffer = "Hello, world!".as_bytes().to_vec();
        let hash = get_md5_hash(&buffer);
        assert_eq!(hash, "6cd3556deb0da54bca060b4c39479839");
    }

    #[test]
    fn test_get_md5_hash_empty() {
        let buffer = "".as_bytes().to_vec();
        let hash = get_md5_hash(&buffer);
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_start_search() {
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let result = start_search(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_search_bad_path() {
        let args = Args {
            path: "data-badpath!!!".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
            verbose: false,
            wildcard: "*".to_string(),
        };
        let result = start_search(&args);
        assert!(result.is_err());
    }
}
