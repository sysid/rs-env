" vim -S openfiles.vim
" openfiles.vim

" Open the first set of files (those containing 'test.env') in the first column
edit a_test.env
split b_test.env
split test.env

split a_int.env
" move to right column
wincmd L
split b_int.env
split int.env

split a_prod.env
wincmd L
split b_prod.env
"split prod.env

" make distribution equal
wincmd =

" jumpt to left top corner
1wincmd w

" Move cursor back to the top-left window
"wincmd H
"wincmd k
