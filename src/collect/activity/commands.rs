//! Program lists behind `command_station` (T41).

/// Programs that never decide a station (`cd`, `echo`…).
pub(super) fn is_neutral(program: &str) -> bool {
    matches!(
        program,
        "cd" | "echo"
            | "printf"
            | "export"
            | "true"
            | "false"
            | "set"
            | "pwd"
            | "sleep"
            | "which"
            | "test"
            | "["
            | "exit"
    )
}

/// A test, lint or check run.
pub(super) fn is_check(program: &str, args: &[&str]) -> bool {
    match program {
        "cargo" => {
            matches!(args.first(), Some(&"test" | &"nextest" | &"clippy" | &"check"))
                || (args.first() == Some(&"fmt") && args.contains(&"--check"))
        }
        "bash" | "sh" => args.first().is_some_and(|s| s.ends_with("check")),
        "check" => true,
        "make" => matches!(args.first(), Some(&"test" | &"check" | &"lint")),
        "pytest" | "ruff" | "mypy" | "eslint" | "tsc" => true,
        "go" => args.first() == Some(&"test"),
        "npm" => {
            args.first() == Some(&"test")
                || (args.first() == Some(&"run")
                    && args.get(1).copied().is_some_and(|s| matches!(s, "test" | "lint" | "check")))
        }
        _ => false,
    }
}

/// A command that talks to the network.
pub(super) fn is_web(program: &str, args: &[&str]) -> bool {
    match program {
        "curl" | "wget" | "gh" | "ssh" | "scp" | "rsync" | "ping" => true,
        "git" => matches!(args.first(), Some(&"push" | &"pull" | &"fetch" | &"clone" | &"ls-remote")),
        "cargo" => matches!(args.first(), Some(&"add" | &"install" | &"update" | &"fetch" | &"publish")),
        "npm" | "pnpm" | "yarn" => {
            matches!(args.first(), Some(&"install" | &"i" | &"ci" | &"add" | &"update"))
        }
        "pip" | "pip3" => matches!(args.first(), Some(&"install" | &"download")),
        _ => false,
    }
}

/// A command that reads or moves files.
pub(super) fn is_file(program: &str) -> bool {
    matches!(
        program,
        "cat"
            | "head"
            | "tail"
            | "sed"
            | "grep"
            | "rg"
            | "find"
            | "ls"
            | "wc"
            | "cp"
            | "mv"
            | "mkdir"
            | "touch"
            | "diff"
            | "cmp"
            | "sort"
            | "uniq"
            | "cut"
            | "tr"
            | "awk"
            | "tee"
            | "stat"
            | "file"
            | "du"
            | "tree"
            | "ln"
            | "rm"
            | "rmdir"
            | "realpath"
            | "basename"
            | "dirname"
            | "jq"
    )
}
