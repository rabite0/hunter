function hunter() {
	env hunter
	test -e ~/.hunter_cwd &&
	source ~/.hunter_cwd &&
	rm ~/.hunter_cwd && cd $HUNTER_CWD
}
