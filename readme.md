# Dexy

Dexy is a command line utility for recursively generating sha256/512 hashes of all files in a directory.

Dexy will output two JSON files, one a mapping of hashes to file path, the other list of all duplicate files that were found in the selected directory.

Note that Dexy has several limitations, feel free to submit a PR to fix them:
- You must edit the source for mundane things, such as the number of threads
- Relative file links are not allowed, they must be absolute
- Duplicate files will match on two empty files
- Panics on broken symlinks
- Only tested on Linux


```
USAGE:
    dexy [OPTIONS] <START_DIRECTORY>...

ARGS:
    <START_DIRECTORY>...    List of directories to scan

OPTIONS:
    -e, --exclude <EXCLUDE>
            NOT IMPLEMENTED: Any directory or file that matches this filter will be excluded,
            supports regex to match against

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

    -u, --update-existing
            NOT IMPLEMENTED: Update an existing scan with files that aren't already present. Will
            attempt to check size and age of existing scanned files and rehash - but note that this
            isn't perfect and it's possible that a file might be missed if it has the same size. If
            this is a critical application, it is recommended that you rescan from scratch

    -V, --version
            Print version information
```