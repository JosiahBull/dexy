# Dexy

Dexy is a command line utility for recursively generating sha256 hashes of all files in a directory.

Dexy will JSON file containing the hashes of all files that were found, note that on slower media such as hard drives the scan may take quote some time. 

If there are multiple files that have the same hash, they will be grouped together in a single entry

## Example Usage
```bash
    cargo run --release -- --ignore-empty --load-file-attributes --name docs /home/$USER/Documents
```
## Example Output
```json
{
  "3e155b0d8756c752021b64e8d39ac7d73dd9e451e55bdfc70d231af773c3b813": [
    {
      "hash": "3e155b0d8756c752021b64e8d39ac7d73dd9e451e55bdfc70d231af773c3b813",
      "path": "/home/josiah/Documents/rust-chat-app/target/doc/itertools/structs/struct.PadUsing.html",
      "attributes": {
        "size": 405813,
        "created_date": 1639433284,
        "accessed_date": 1650427097,
        "edit_date": 1639433284,
        "file_type": "File"
      }
    }
  ],
 }
```


## Full Avaiable Options
```
USAGE:
    dexy [OPTIONS] <START_DIRECTORY>...

ARGS:
    <START_DIRECTORY>...    List of directories to scan

OPTIONS:
    -h, --help
            Print help information

    -i, --ignore-empty
            Whether empty files (e.g. files with 0 bytes) should be ignored. This is primarily
            useful for avoiding many ""duplicate"" empty files

        --include-hidden
            By default the program will exclude hidden files/folders, this will force it to include
            them

    -l, --load-file-attributes
            Output size and other file information with the scan, note this makes an extra request
            to the underlying system, so may add some time to the inital scan

    -n, --name <NAME>
            Name of the scan, this will be used to name the output files [default: dexy]

    -o, --out <OUT>
            Output Directory [default: ./]

    -t, --thread-count <THREAD_COUNT>
            Number of threads to process default = number of cores [default: 16]

    -V, --version
            Print version information
```
