# rs-env

Build environment variable set from a hierarchical list of <name.env> files.
Each file can point to a parent which will be loaded first.
Last defined variable wins, i.e. child trumps parent.

```bash
source <(rsenv build)
```
