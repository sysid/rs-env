# Storyline rsenv

## Prep
- make font much bigger !!!
- clean .envrc
- rm *.env

## Run
```bash
asciinema rec --overwrite -i 3 /tmp/rsenv.cast

rsenv
cls

vim -o root.env l1.env l2.env
rsenv build l2.env
rsenv files l2.env
rsenv edit .
rm *.env


rsenv
cls
tree

more .envrc
rsenv build environments/complex/
rsenv envrc environments/complex/level4.env
more .envrc

cls; asciinema rec --overwrite -i 3 /tmp/rsenv3.cast
rsenv tree environments/parallel/
rsenv tree-edit environments/parallel/
# replace q-nr

cls; asciinema rec --overwrite -i 3 /tmp/rsenv4.cast
cls
rsenv
more .envrc
rsenv select .  # prod
more .envrc
```

## Close

CTRL-D
