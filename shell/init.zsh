work() {
    local eval_file
    eval_file=$(mktemp)
    WORK_SHELL=zsh WORK_SHELL_EVAL="$eval_file" command work "$@"
    local s=$?
    if [ -s "$eval_file" ]; then
        source "$eval_file"
    fi
    rm -f "$eval_file"
    return $s
}
