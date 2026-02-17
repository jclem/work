function work --wraps work
    set -l eval_file (mktemp)
    WORK_SHELL=fish WORK_SHELL_EVAL=$eval_file command work $argv
    set -l s $status
    if test -s $eval_file
        source $eval_file
    end
    rm -f $eval_file
    return $s
end
