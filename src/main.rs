use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::RwLock;

use clap::{Parser, ValueHint};

/// An application to recursively scan a directory generating sha256 hashes for all contained
/// files, and outputing the result to JSON.
#[derive(Parser, Debug, PartialEq)]
#[clap(author, version, about, long_about = None, trailing_var_arg = true,)]
struct Args {
    /// List of directories to scan
    #[clap(required = true, min_values = 1, value_hint = ValueHint::AnyPath)]
    start_directory: Vec<PathBuf>,

    /// Output Directory
    #[clap(short, long, default_value = "./", value_hint = ValueHint::DirPath)]
    out: PathBuf,

    /// Name of the scan, this will be used to name the output files
    #[clap(short, long, default_value = "dexy")]
    name: String,

    /// NOT IMPLEMENTED: Any directory or file that matches this filter will be excluded,
    /// supports regex to match against.
    #[clap(short, long, value_hint = ValueHint::DirPath)]
    exclude: Vec<PathBuf>,

    /// Number of threads to process
    /// default = number of cores
    #[clap(short, long, default_value_t = num_cpus::get())]
    thread_count: usize,

    /// Whether empty files (e.g. files with 0 bytes) should be ignored. This is primarily
    /// useful for avoiding many ""duplicate"" empty files.
    #[clap(short, long)]
    ignore_empty: bool,

    /// By default the program will exclude hidden files/folders, this will force it to include them.
    #[clap(long)]
    include_hidden: bool,

    /// Output size and other file information with the scan, note this makes an extra
    /// request to the underlying system, so may add some time to the inital scan.
    #[clap(short, long)]
    load_file_attributes: bool,

    /// NOT IMPLEMENTED: Update an existing scan with files that aren't already present. Will attempt to check
    /// size and age of existing scanned files and rehash - but note that this isn't perfect and it's
    /// possible that a file might be missed if it has the same size. If this is a critical
    /// application, it is recommended that you rescan from scratch.
    #[clap(short, long)]
    update_existing: bool,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize)]
struct ScannedFile {
    /// The generated hash for this file
    hash: String,
    /// The path to this file
    path: PathBuf,
    /// Optional File Attributes
    attributes: Option<FileAttributes>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize)]
struct FileAttributes {
    size: usize,
    created_date: i128,
    accessed_date: i128,
    edit_date: i128,
    file_type: FileType,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize)]
enum FileType {
    SymLink,
    Directory,
    File,
}

async fn worker(
    thread: usize,
    dir_queue: Arc<RwLock<VecDeque<PathBuf>>>,
    num_waiting: Arc<AtomicUsize>,
    global_result: Arc<RwLock<HashMap<String, Vec<ScannedFile>>>>,
    progressbar: ProgressBar,
    main_pb: Arc<RwLock<ProgressBar>>,
    args: Arc<Args>,
) {
    progressbar
        .set_style(ProgressStyle::default_spinner().template("{spinner} {prefix}: {wide_msg}"));
    progressbar.set_prefix(format!("{}", thread + 1));
    progressbar.set_message("started");

    let mut waiting = false;

    while num_waiting.load(Ordering::Relaxed) != args.thread_count {
        let item = dir_queue.write().await.pop_front();
        if let Some(path) = item {
            progressbar.set_message(format!("Processing dir: {:?}", &path));
            if waiting {
                num_waiting.fetch_sub(1, Ordering::Relaxed);
                waiting = false;
            }

            let mut folders: Vec<PathBuf> = vec![];
            let mut result: HashMap<String, Vec<ScannedFile>> = HashMap::default();
            let mut fs = match tokio::fs::read_dir(&path).await {
                Ok(dir) => dir,
                Err(e) => {
                    progressbar.println(format!(
                        "Error: {} {}",
                        e,
                        path.to_string_lossy()
                    ));
                    continue;
                }
            };

            while let Ok(Some(s)) = fs.next_entry().await {
                //XXX: there may be a better way to find hidden files?
                //XXX: Windows support?
                if !args.include_hidden && s.path().to_str().unwrap().contains("/.") {
                    progressbar.println(format!(
                        "Skipped hidden path: {}",
                        s.path().to_string_lossy()
                    ));
                    continue;
                }

                if s.path().is_dir() {
                    folders.push(s.path());
                } else {
                    progressbar
                        .set_message(format!("Scanning file: {}", &s.path().to_string_lossy()));
                    //open file
                    let internal_path = s.path();

                    //check if is symlink, and if symlink is broken
                    let metadata = match tokio::fs::symlink_metadata(&internal_path).await {
                        Ok(m) => m,
                        Err(_) => {
                            progressbar.println(format!(
                                "Skipped broken symlink: {}",
                                internal_path.to_string_lossy()
                            ));
                            continue;
                        }
                    };

                    if args.ignore_empty && metadata.len() == 0 {
                        continue; //Skip empty files
                    }

                    let file = match tokio::fs::File::open(&internal_path).await {
                        Ok(f) => f,
                        Err(e) => {
                            progressbar.println(format!(
                                "Error: {} {}",
                                e,
                                internal_path.to_string_lossy()
                            ));
                            continue;
                        }
                    };

                    let mut hasher_file = file.into_std().await;
                    let hasher: Result<Sha256, std::io::Error> =
                        tokio::task::spawn_blocking(move || {
                            let mut hasher = Sha256::new();
                            std::io::copy(&mut hasher_file, &mut hasher)?;
                            Ok(hasher)
                        })
                        .await
                        .unwrap();

                    let hasher = match hasher {
                        Ok(f) => f,
                        Err(e) => {
                            progressbar.println(format!(
                                "Cannot generate hash: {} {}",
                                internal_path.to_string_lossy(),
                                e
                            ));
                            continue;
                        }
                    };

                    let hash = format!("{:x}", hasher.finalize());

                    let attributes = match args.load_file_attributes {
                        true => Some(FileAttributes {
                            size: metadata.len() as usize,
                            created_date: match metadata.created() {
                                Ok(f) => f
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .expect("time went backwards")
                                    .as_secs() as i128,
                                Err(_) => -1,
                            },
                            accessed_date: match metadata.accessed() {
                                Ok(f) => f
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .expect("time went backwards")
                                    .as_secs() as i128,
                                Err(_) => -1,
                            },
                            edit_date: match metadata.modified() {
                                Ok(f) => f
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .expect("time went backwards")
                                    .as_secs() as i128,
                                Err(_) => -1,
                            },
                            file_type: {
                                if metadata.is_symlink() {
                                    FileType::SymLink
                                } else if metadata.is_dir() {
                                    FileType::Directory
                                } else {
                                    FileType::File
                                }
                            },
                        }),
                        false => None,
                    };

                    let scanned_file = ScannedFile {
                        hash,
                        path: s.path(),
                        attributes,
                    };

                    let contains_res = result.contains_key(&scanned_file.hash);
                    if contains_res {
                        result
                            .get_mut(&scanned_file.hash)
                            .unwrap()
                            .push(scanned_file);
                    } else {
                        result.insert(scanned_file.hash.clone(), vec![scanned_file]);
                    }
                }
            }

            if !folders.is_empty() {
                dir_queue.write().await.extend(folders.into_iter());
            }

            if !result.is_empty() {
                global_result.write().await.extend(result.into_iter());
            }

            let pb = main_pb.write().await;
            pb.inc(1);
            pb.set_length(dir_queue.read().await.len() as u64 + pb.position());
        } else {
            progressbar.set_message("Waiting for new tasks");
            if !waiting {
                num_waiting.fetch_add(1, Ordering::Relaxed);
                waiting = true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await; //Wait for new tasks to appear
        }
    }

    progressbar.finish_with_message("closing...");
    if thread == 0 {
        main_pb.write().await.finish();
    }
}

#[tokio::main]
async fn main() {
    let args = Arc::new(Args::parse());

    //TODO: - allow "grep" patterns

    let result = Arc::new(RwLock::new(HashMap::default()));
    let queue = Arc::new(RwLock::new(VecDeque::new()));
    let counter = Arc::new(AtomicUsize::new(0));

    // // If updating, we should load the existing data
    // if args.update_existing {
    //     let data = tokio::fs::read_to_string(format!(
    //         "{}.json",
    //         args.out.join(args.name.clone()).to_string_lossy()
    //     )).await.expect("able to read existing file");


    // }

    println!(
        "starting at: {}",
        &args.start_directory[0].to_string_lossy()
    );

    queue.write().await.extend(    args.start_directory.iter().map(|x| {
        x.canonicalize().expect("able to canonicalize provided path")
    }));

    let progressbar = MultiProgress::new();
    let main_pb = Arc::new(RwLock::new(progressbar.add(ProgressBar::new(1))));
    main_pb.write().await.set_style(
        ProgressStyle::default_bar()
            .template(
                "[{elapsed}]/[{eta}] {wide_bar:.cyan/blue} {pos:>7}/{len:7} {msg}",
            )
            .progress_chars("##-"),
    );

    let mut handles = vec![];
    for i in 0..args.thread_count {
        let thread_pb = progressbar.insert(0, ProgressBar::new(0));
        let handle = tokio::spawn(worker(
            i,
            queue.clone(),
            counter.clone(),
            result.clone(),
            thread_pb,
            main_pb.clone(),
            args.clone(),
        ));
        handles.push(handle);
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    progressbar.join().unwrap();

    futures::future::join_all(handles).await;

    // Finished processing
    // Write hashes
    let data = result.read().await;
    tokio::fs::write(
        format!(
            "{}.json",
            args.out.join(args.name.clone()).to_string_lossy()
        ),
        serde_json::to_string(&*data).unwrap(),
    )
    .await
    .unwrap();
}
