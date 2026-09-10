# Install
If you do not have the binary get the latest release [here](https://github.com/dhogenson/flanix/releases)

# Basic use

Add a folder you want to sync
```bash
flanix add <namespace> <path/to/push> 
# Example:
flanix add docs ~/Documents
```
If you want to push your files to the cloud
```bash
flanix push <namespace>
# Example:
flanix push docs
```
And if you want to sync your files from the cloud
```bash
flanix pull <namespace>
# Example:
flanix pull docs
```
