# rs-env

Build environment variable set from a hierarchical list of <name.env> files.
Each file can point to a parent which will be loaded first.
Last defined variable wins, i.e. child trumps parent.

```bash
source <(rsenv build)
```

# Intellij: Life injection of environment variables
Plugin "EnvFile" can be used to life-inject environment variables.
Use the script `runenv.sh` as the "EnvFile" script (tick executable checkbox !).
The environment variable `RUN_ENV` and will tell the script which environment to load.
It will look for a file `<RUN_ENV>.env` in the specified directory.


# Development
Tests for "skim" need valid terminal, so they are run via Makefile.
Test for `rsenv select`: run debug target and check rsenv .envrc file.
