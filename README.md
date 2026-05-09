## Searcher
Searcher is a fast command-line tool written in Rust for searching files and folders across local paths and Windows SMB shares.
### Features
- Search files, folders or both.
- Supports local paths and UNC paths.
- Supports multiple search roots.
- Multi-threaded directory traversal.
- Configurable depth and worker threads.
- Quiet mode for clean output.
### Usage
```powershell
search.exe all "\\server\share" revit -d 5 -t 20
search.exe folder "\\localhost" main -q
search.exe file "C:\" main --depth 10 --threads 50
````
### Modes
```
file    Search only files
folder  Search only folders
all     Search files and folders
```
### Options
```
-d, --depth <DEPTH>       Maximum search depth
-t, --threads <THREADS>   Number of worker threads
-q, --quiet              
```
### Notes
This tool is intended for legitimate administrative and inventory tasks in local or authorized environments. Use conservative thread counts when scanning large SMB shares.
