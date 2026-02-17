use crate::cli::InitShell;

pub fn run(shell: InitShell) {
    let script = match shell {
        InitShell::Fish => include_str!("../../shell/init.fish"),
        InitShell::Bash => include_str!("../../shell/init.bash"),
        InitShell::Zsh => include_str!("../../shell/init.zsh"),
    };
    print!("{script}");
}
