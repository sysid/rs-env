# rs-env

Build environment variable set from a hierarchical list of <name.env> files.
Each file can point to a parent which will be loaded first.
Last defined variable wins, i.e. child trumps parent.

```bash
source <(rsenv build)
```


# Development
Tests for "skim" need valid terminal, so they are run via Makefile.
Test for `rsenv select`: run debug target and check rsenv .envrc file.
