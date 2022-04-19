# Dexy

Dexy is a command line utility for recursively generating sha256/512 hashes of all files in a directory.

Dexy will output two JSON files, one a mapping of hashes to file path, the other list of all duplicate files that were found in the selected directory.

Note that Dexy has several limitations, feel free to submit a PR to fix them:
- You must edit the source for mundane things, such as the number of threads
- Relative file links are not allowed, they must be absolute
- Duplicate files will match on two empty files
- Panics on broken symlinks
- Only tested on Linux

Usage:
```
    cargo build --release
    ./target/release/dexy /home/$USER home_hashes
```