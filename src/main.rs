use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    env,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::RwLock;

const NUM_THREADS: usize = 8;

async fn worker(
    thread: usize,
    file_queue: Arc<RwLock<VecDeque<PathBuf>>>,
    num_waiting: Arc<AtomicUsize>,
    global_result: Arc<RwLock<HashMap<String, String>>>,
    global_duplicates: Arc<RwLock<Vec<(String, String, String)>>>,
    progressbar: ProgressBar,
    main_pb: Arc<RwLock<ProgressBar>>,
) {
    progressbar
        .set_style(ProgressStyle::default_spinner().template("{spinner} {prefix}: {wide_msg}"));
    progressbar.set_prefix(format!("{}", thread + 1));
    progressbar.set_message("started");

    let mut waiting = false;

    while num_waiting.load(Ordering::Relaxed) != NUM_THREADS {
        let item = file_queue.write().await.pop_front();
        if let Some(path) = item {
            progressbar.set_message(format!("Processing dir: {:?}", &path));
            if waiting {
                num_waiting.fetch_sub(1, Ordering::Relaxed);
                waiting = false;
            }

            let mut folders: Vec<PathBuf> = vec![];
            let mut result: HashMap<String, String> = HashMap::default();
            let mut duplicates = vec![];
            let mut fs = tokio::fs::read_dir(&path).await.unwrap();

            while let Ok(Some(s)) = fs.next_entry().await {
                if s.path().is_dir() {
                    folders.push(s.path());
                } else {
                    progressbar.set_message(format!("Scanning file: {}", &s.path().to_string_lossy()));
                    //open file
                    let internal_path = s.path();
                    let hasher = tokio::task::spawn_blocking(move || {
                        let mut file = std::fs::File::open(internal_path).unwrap();
                        let mut hasher = Sha256::new();
                        std::io::copy(&mut file, &mut hasher).unwrap();
                        hasher
                    })
                    .await
                    .unwrap();

                    let hash = format!("{:x}", hasher.finalize());
                    let existing_item =
                        result.insert(hash.clone(), s.path().to_string_lossy().to_string());
                    if let Some(ex) = existing_item {
                        duplicates.push((hash, ex, s.path().to_string_lossy().to_string()));
                    }
                }
            }

            if !folders.is_empty() {
                file_queue.write().await.extend(folders.into_iter());
            }

            if !result.is_empty() {
                global_result.write().await.extend(result.into_iter());
            }

            if !duplicates.is_empty() {
                global_duplicates
                    .write()
                    .await
                    .extend(duplicates.into_iter());
            }

            let pb = main_pb.write().await;
            pb.inc(1);
            pb.set_length(file_queue.read().await.len() as u64 + pb.position());
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
    //XXX: add clap to make this a proper cli tool
    //- allow dynamic selection of thread count
    //- allow relative links
    //- fully validate "duplicate" files by comparison
    //- duplicate file struct output
    //- handle broken symlinks etc
    //- validate for windows
    //- test
    let result = Arc::new(RwLock::new(HashMap::default()));
    let queue = Arc::new(RwLock::new(VecDeque::new()));
    let duplicates = Arc::new(RwLock::new(vec![]));
    let counter = Arc::new(AtomicUsize::new(0));
    let start_file: String = env::args().nth(1).unwrap();
    let out_name: String = env::args().nth(2).unwrap();

    println!("starting at: {start_file}");
    queue.write().await.push_back(PathBuf::from(start_file));

    let progressbar = MultiProgress::new();

    let main_pb = Arc::new(RwLock::new(progressbar.add(ProgressBar::new(1))));

    main_pb.write().await.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed}]/[{per_sec}]/[{eta}] {wide_bar:.cyan/blue} {pos:>7}/{len:7} {msg}")
            .progress_chars("##-"),
    );

    let mut handles = vec![];
    for i in 0..NUM_THREADS {
        let thread_pb = progressbar.insert(0, ProgressBar::new(0));
        let handle = tokio::spawn(worker(
            i,
            queue.clone(),
            counter.clone(),
            result.clone(),
            duplicates.clone(),
            thread_pb,
            main_pb.clone(),
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
        format!("{out_name}.json"),
        serde_json::to_string(&*data).unwrap(),
    )
    .await
    .unwrap();

    //Write duplicates
    let data = duplicates.read().await;
    tokio::fs::write(
        format!("{out_name}-dups.json"),
        serde_json::to_string(&*data).unwrap(),
    )
    .await
    .unwrap();
}
