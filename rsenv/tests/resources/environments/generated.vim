" Open the first set of files ('a_test.env') in the first column
edit a_test.env
split b_test.env
split test.env
split a_int.env
" move to right column
wincmd L
split b_int.env
split int.env
split a_prod.env
" move to right column
wincmd L

" make distribution equal
wincmd =

" jump to left top corner
1wincmd w
