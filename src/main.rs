
use chrono::{DateTime, Utc};
use clap::Parser;
use indicatif::ProgressBar;
use md5::{self, Digest};
use std::io::Read;
use std::{fs, thread};
use std::time::UNIX_EPOCH;
use std::{collections::HashMap, time::Duration};

#[derive(Parser, Debug)]
#[command(name = "Dupefindr", version)]
#[command(about = "A tool to find duplicate files", long_about = None)]
#[command(propagate_version = true)]
struct Args {
    /// The directory to search for duplicates in.
    #[arg(short, long, default_value = ".")]
    path: String,

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
}

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
        println!("Include 0 byte files: {}", args.include_zero_byte_files);
        println!("Dry run: {}", args.dry_run);
        println!("Include hidden files: {}", args.include_hidden_files);
    }
    args
}
fn start_search(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let bar = ProgressBar::new_spinner().with_message("Collecting files...");
    bar.enable_steady_tick(Duration::from_millis(100));
    // get the files in the directory
    let folder_path: String = args.path.clone();
    let mut hash_map: HashMap<String, Vec<FileInfo>> = HashMap::new();
    let _result = get_files_in_directory(args, folder_path, &bar);
    let _files = match _result {
        Ok(files) => files,
        Err(e) => {
            bar.println(format!("Error: {}", e));
            return Err(e);
        }
    };
    if args.debug {
        bar.println(format!("Found {} files", _files.len()));
    }
    bar.set_message("Identifying duplicates...");
    for file in _files {
        let hash_string = get_hash_of_file(&file.path, &bar);
        if args.debug {
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
        if args.debug {
            thread::sleep(Duration::from_millis(500));
        }
    }
    // print the duplicates
    for (hash, files) in hash_map.iter() {
        if files.len() > 1 {
            bar.println(format!(
                "Found {} duplicates for hash: {}",
                files.len(),
                hash
            ));
            for file in files {
                bar.println(format!(
                    "File: {} [created: {}] [modified: {}] [{} bytes]",
                    file.path,
                    file.created_at.to_rfc2822(),
                    file.modified_at.to_rfc2822(),
                    file.size
                ));
            }
        }
    }
    bar.finish_and_clear();
    Ok(())
}

fn get_files_in_directory(args: &Args, folder_path: String, bar: &ProgressBar) -> Result<Vec<FileInfo>, Box<dyn std::error::Error>> {
    let mut files: Vec<FileInfo> = Vec::new();
    let dir_path = std::path::Path::new(folder_path.as_str());
    match fs::metadata(folder_path.as_str()) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(Box::<dyn std::error::Error>::from("The path provided is not a directory"));
            }
        }
        Err(_) => {
            return Err(Box::from("The path provided is not a directory"));
        }
    }
    let total_files = std::fs::read_dir(dir_path).unwrap().count();
    bar.println(format!("Searching in: {} - {} objects", folder_path, total_files));
    for entry in std::fs::read_dir(dir_path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            if !args.recursive {
                if args.debug {
                    bar.println(format!("Ignoring directory: {}", path.to_str().unwrap()));
                    thread::sleep(Duration::from_millis(500));
                }
                continue;
            }
            else {
                let sub_files = get_files_in_directory(args, path.to_str().unwrap().to_string(), bar)?;
                files.extend(sub_files);
            }
        }
        if path.is_file() {
            let hidden: bool;
            #[cfg(not(target_os = "windows"))]
            {
                hidden = path.file_name().unwrap().to_str().unwrap().starts_with(".");
            }
            #[cfg(target_os = "windows")]
            {
                if std::fs::metadata(&path).unwrap().file_attributes().hidden().unwrap() {
                    hidden = true;
                }
            }
            if args.include_hidden_files == false && hidden {
                if args.debug {
                    bar.println(format!("Ignoring hidden file: {}", path.to_str().unwrap()));
                    thread::sleep(Duration::from_millis(500));
                }
                continue;
            }
            let size = std::fs::metadata(&path).unwrap().len();
            let created_at = std::fs::metadata(&path).unwrap().created().unwrap();
            let modified_at = std::fs::metadata(&path).unwrap().modified().unwrap();
            // Convert SystemTime to chrono::DateTime<Utc>
            let created_at_utc_datetime: DateTime<Utc> = DateTime::from(UNIX_EPOCH)
                + chrono::Duration::from_std(created_at.duration_since(UNIX_EPOCH).unwrap())
                    .unwrap();
            let modified_at_utc_datetime: DateTime<Utc> = DateTime::from(UNIX_EPOCH)
                + chrono::Duration::from_std(modified_at.duration_since(UNIX_EPOCH).unwrap())
                    .unwrap();

            if size == 0 && !args.include_zero_byte_files {
                if args.debug {
                    bar.println(format!("Ignoring 0 byte file: {}", path.to_str().unwrap()));
                    thread::sleep(Duration::from_millis(500));
                }
                continue;
            }
            let file_info = FileInfo {
                path: path.to_str().unwrap().to_string(),
                size,
                created_at: created_at_utc_datetime,
                modified_at: modified_at_utc_datetime,
            };
            files.push(file_info);
            if args.debug {
                thread::sleep(Duration::from_millis(500));
            }
        }
    }

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
    fn test_get_command_line_arguments() {
        // test default values for cmd args
        let args = get_command_line_arguments();
        assert_eq!(args.path, ".");
        assert_eq!(args.recursive, false);
        assert_eq!(args.debug, true);
        assert_eq!(args.include_zero_byte_files, false);
        assert_eq!(args.dry_run, false);
        assert_eq!(args.include_hidden_files, false);
    }

    #[test]
    fn test_get_files_in_directory() {
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
        };
        let files = get_files_in_directory(&args, "data".to_string(), &ProgressBar::new_spinner()).unwrap();
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn test_get_files_in_directory_include_empty() {
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: true,
            dry_run: false,
            include_hidden_files: false,
        };
        let files = get_files_in_directory(&args, "data".to_string(), &ProgressBar::new_spinner()).unwrap();
        assert_eq!(files.len(), 7);
    }

    #[test]
    fn test_get_files_in_directory_include_hidden() {
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: true,
        };
        let files = get_files_in_directory(&args, "data".to_string(), &ProgressBar::new_spinner()).unwrap();
        assert_eq!(files.len(), 6);
    }

    #[test]
    fn test_get_files_in_directory_include_all_files() {
        let args = Args {
            path: "data".to_string(),
            recursive: false,
            debug: false,
            include_zero_byte_files: true,
            dry_run: false,
            include_hidden_files: true,
        };
        let files = get_files_in_directory(&args, "data".to_string(), &ProgressBar::new_spinner()).unwrap();
        assert_eq!(files.len(), 8);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive() {
        let args = Args {
            path: "data".to_string(),
            recursive: true,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
        };
        let files = get_files_in_directory(&args, "data".to_string(), &ProgressBar::new_spinner()).unwrap();
        assert_eq!(files.len(), 16);
    }

    #[test]
    fn test_get_files_in_directory_include_recursive_with_hidden() {
        let args = Args {
            path: "data".to_string(),
            recursive: true,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: true,
        };
        let files = get_files_in_directory(&args, "data".to_string(), &ProgressBar::new_spinner()).unwrap();
        assert_eq!(files.len(), 18);
    }

    #[test]
    fn test_get_files_in_directory_bad_path() {
        let args = Args {
            path: "badpath!!!".to_string(),
            recursive: true,
            debug: false,
            include_zero_byte_files: false,
            dry_run: false,
            include_hidden_files: false,
        };
        let result = get_files_in_directory(&args, "badpath!!!".to_string(), &ProgressBar::new_spinner());
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
        };
        let result = start_search(&args);
        assert!(result.is_err());
    }

}