use std::io::Write;

use proc_exit::WithCodeResultExt;
use structopt::StructOpt;

fn main() {
    human_panic::setup_panic!();
    let result = run();
    proc_exit::exit(result);
}

fn run() -> proc_exit::ExitResult {
    // clap's `get_matches` uses Failure rather than Usage, so bypass it for `get_matches_safe`.
    let args = match Args::from_args_safe() {
        Ok(args) => args,
        Err(e) if e.use_stderr() => {
            return Err(proc_exit::Code::USAGE_ERR.with_message(e));
        }
        Err(e) => {
            writeln!(std::io::stdout(), "{}", e)?;
            return proc_exit::Code::SUCCESS.ok();
        }
    };

    let colored = args.color.colored().or_else(git_stack::color::colored_env);
    let mut colored_stdout = colored
        .or_else(git_stack::color::colored_stdout)
        .unwrap_or(true);
    let mut colored_stderr = colored
        .or_else(git_stack::color::colored_stderr)
        .unwrap_or(true);
    if (colored_stdout || colored_stderr) && !yansi::Paint::enable_windows_ascii() {
        colored_stdout = false;
        colored_stderr = false;
    }

    git_stack::log::init_logging(args.verbose.clone(), colored_stderr);

    if let Some(output_path) = args.dump_config.as_deref() {
        dump_config(&args, output_path)?;
    } else if let Some(ignore) = args.protect.as_deref() {
        protect(&args, ignore)?;
    } else if args.show {
        show(&args, colored_stdout)?;
    } else {
        rewrite(&args)?;
    }

    Ok(())
}

fn dump_config(_args: &Args, output_path: &std::path::Path) -> proc_exit::ExitResult {
    log::trace!("Initializing");
    let cwd = std::env::current_dir().with_code(proc_exit::Code::USAGE_ERR)?;
    let repo = git2::Repository::discover(&cwd).with_code(proc_exit::Code::USAGE_ERR)?;

    let repo_config =
        git_stack::config::RepoConfig::from_all(&repo).with_code(proc_exit::Code::CONFIG_ERR)?;

    let output = toml::to_string_pretty(&repo_config).with_code(proc_exit::Code::FAILURE)?;

    if output_path == std::path::Path::new("-") {
        std::io::stdout().write_all(output.as_bytes())?;
    } else {
        std::fs::write(output_path, &output)?;
    }

    Ok(())
}

fn protect(_args: &Args, ignore: &str) -> proc_exit::ExitResult {
    log::trace!("Initializing");
    let cwd = std::env::current_dir().with_code(proc_exit::Code::USAGE_ERR)?;
    let repo = git2::Repository::discover(&cwd).with_code(proc_exit::Code::USAGE_ERR)?;

    let mut repo_config =
        git_stack::config::RepoConfig::from_repo(&repo).with_code(proc_exit::Code::CONFIG_ERR)?;
    repo_config
        .protected_branches
        .get_or_insert_with(Vec::new)
        .push(ignore.to_owned());

    repo_config
        .write_repo(&repo)
        .with_code(proc_exit::Code::FAILURE)?;

    Ok(())
}

fn show(args: &Args, colored_stdout: bool) -> proc_exit::ExitResult {
    log::trace!("Initializing");
    let cwd = std::env::current_dir().with_code(proc_exit::Code::USAGE_ERR)?;
    let repo = git2::Repository::discover(&cwd).with_code(proc_exit::Code::USAGE_ERR)?;

    let repo_config =
        git_stack::config::RepoConfig::from_all(&repo).with_code(proc_exit::Code::CONFIG_ERR)?;
    let protected = git_stack::protect::ProtectedBranches::new(
        repo_config
            .protected_branches
            .iter()
            .flatten()
            .map(|s| s.as_str()),
    )
    .with_code(proc_exit::Code::CONFIG_ERR)?;
    let branches =
        git_stack::branches::Branches::new(&repo).with_code(proc_exit::Code::CONFIG_ERR)?;

    let protected_branches = branches.protected(&repo, &protected);

    let head_oid = git_stack::git::head_oid(&repo).with_code(proc_exit::Code::USAGE_ERR)?;

    let base_branch = match args.base.as_deref() {
        Some(branch_name) => git_stack::git::resolve_branch(&repo, branch_name)
            .with_code(proc_exit::Code::USAGE_ERR)?,
        None => {
            let branch =
                git_stack::branches::find_protected_base(&repo, &protected_branches, head_oid)
                    .with_code(proc_exit::Code::USAGE_ERR)?;
            log::debug!(
                "Chose branch {} as the base",
                branch
                    .name()
                    .ok()
                    .flatten()
                    .unwrap_or(git_stack::git::NO_BRANCH)
            );
            git_stack::branches::clone_local_branch(&repo, branch)
                .expect("previously confirmed to be valid")
        }
    };

    let base_oid = base_branch
        .get()
        .target()
        .ok_or_else(|| {
            git2::Error::new(
                git2::ErrorCode::NotFound,
                git2::ErrorClass::Reference,
                format!(
                    "could not resolve {}",
                    base_branch
                        .name()
                        .ok()
                        .flatten()
                        .unwrap_or(git_stack::git::NO_BRANCH)
                ),
            )
        })
        .with_code(proc_exit::Code::USAGE_ERR)?;
    let merge_base_oid = repo
        .merge_base(base_oid, head_oid)
        .with_code(proc_exit::Code::USAGE_ERR)?;

    let graphed_branches = if args.all {
        branches.all(&repo)
    } else if args.dependents {
        branches.dependents(&repo, merge_base_oid, head_oid)
    } else {
        branches.branch(&repo, merge_base_oid, head_oid)
    };

    let mut root = git_stack::dag::graph(
        &repo,
        merge_base_oid,
        head_oid,
        &protected_branches,
        graphed_branches,
    )
    .with_code(proc_exit::Code::CONFIG_ERR)?;
    git_stack::dag::protect_branches(&mut root, &repo, &protected_branches)
        .with_code(proc_exit::Code::CONFIG_ERR)?;

    writeln!(
        std::io::stdout(),
        "{}",
        root.display().colored(colored_stdout).all(args.show_all)
    )?;

    Ok(())
}

fn rewrite(args: &Args) -> proc_exit::ExitResult {
    if args.interactive {
        log::debug!("--interactive is not implemented yet");
    }
    if args.fix {
        log::debug!("--fix is not implemented yet");
    }
    if args.onto.is_some() {
        log::debug!("--onto is not implemented yet");
    }

    log::trace!("Initializing");
    let cwd = std::env::current_dir().with_code(proc_exit::Code::USAGE_ERR)?;
    let repo = git2::Repository::discover(&cwd).with_code(proc_exit::Code::USAGE_ERR)?;

    if git_stack::git::is_dirty(&repo).with_code(proc_exit::Code::USAGE_ERR)? {
        return Err(proc_exit::Code::USAGE_ERR.with_message(format!(
            "Repository at {} is dirty, aborting",
            repo.workdir().unwrap().display()
        )));
    }

    let repo_config =
        git_stack::config::RepoConfig::from_all(&repo).with_code(proc_exit::Code::CONFIG_ERR)?;
    let protected = git_stack::protect::ProtectedBranches::new(
        repo_config
            .protected_branches
            .iter()
            .flatten()
            .map(|s| s.as_str()),
    )
    .with_code(proc_exit::Code::CONFIG_ERR)?;
    let branches =
        git_stack::branches::Branches::new(&repo).with_code(proc_exit::Code::CONFIG_ERR)?;

    let protected_branches = branches.protected(&repo, &protected);

    let head_oid = git_stack::git::head_oid(&repo).with_code(proc_exit::Code::USAGE_ERR)?;

    let base_branch = match args.base.as_deref() {
        Some(branch_name) => git_stack::git::resolve_branch(&repo, branch_name)
            .with_code(proc_exit::Code::USAGE_ERR)?,
        None => {
            let branch =
                git_stack::branches::find_protected_base(&repo, &protected_branches, head_oid)
                    .with_code(proc_exit::Code::USAGE_ERR)?;
            log::debug!(
                "Chose branch {} as the base",
                branch
                    .name()
                    .ok()
                    .flatten()
                    .unwrap_or(git_stack::git::NO_BRANCH)
            );
            git_stack::branches::clone_local_branch(&repo, branch)
                .expect("previously confirmed to be valid")
        }
    };

    let base_oid = base_branch
        .get()
        .target()
        .ok_or_else(|| {
            git2::Error::new(
                git2::ErrorCode::NotFound,
                git2::ErrorClass::Reference,
                format!(
                    "could not resolve {}",
                    base_branch
                        .name()
                        .ok()
                        .flatten()
                        .unwrap_or(git_stack::git::NO_BRANCH)
                ),
            )
        })
        .with_code(proc_exit::Code::USAGE_ERR)?;
    let merge_base_oid = repo
        .merge_base(base_oid, head_oid)
        .with_code(proc_exit::Code::USAGE_ERR)?;

    let graphed_branches = if args.all {
        branches.all(&repo)
    } else if args.dependents {
        branches.dependents(&repo, merge_base_oid, head_oid)
    } else {
        branches.branch(&repo, merge_base_oid, head_oid)
    };

    let mut root = git_stack::dag::graph(
        &repo,
        merge_base_oid,
        head_oid,
        &protected_branches,
        graphed_branches,
    )
    .with_code(proc_exit::Code::CONFIG_ERR)?;
    git_stack::dag::protect_branches(&mut root, &repo, &protected_branches)
        .with_code(proc_exit::Code::CONFIG_ERR)?;
    git_stack::dag::rebase_branches(&mut root, base_oid).with_code(proc_exit::Code::CONFIG_ERR)?;

    println!("{:?}", root);

    Ok(())
}

#[derive(structopt::StructOpt)]
#[structopt(
        setting = structopt::clap::AppSettings::UnifiedHelpMessage,
        setting = structopt::clap::AppSettings::DeriveDisplayOrder,
        setting = structopt::clap::AppSettings::DontCollapseArgsInUsage
    )]
#[structopt(group = structopt::clap::ArgGroup::with_name("mode").multiple(false))]
struct Args {
    /// Show stack relationship
    #[structopt(short, long, group = "mode")]
    show: bool,

    /// Show all commits
    #[structopt(long)]
    show_all: bool,

    /// Write the current configuration to file with `-` for stdout
    #[structopt(long, group = "mode")]
    dump_config: Option<std::path::PathBuf>,

    /// Append a protected branch to the repository's config (gitignore syntax)
    #[structopt(long, group = "mode")]
    protect: Option<String>,

    /// Visually edit history in your $EDITOR`
    #[structopt(short, long)]
    interactive: bool,

    /// Apply all fixups
    #[structopt(long)]
    fix: bool,

    /// Include all dependent branches as well
    #[structopt(short, long)]
    dependents: bool,

    /// Include all branches
    #[structopt(short, long)]
    all: bool,

    /// Branch to evaluate from (default: last protected branch)
    #[structopt(long)]
    base: Option<String>,

    /// Branch to rebase onto (default: base)
    #[structopt(long)]
    onto: Option<String>,

    #[structopt(flatten)]
    color: git_stack::color::ColorArgs,

    #[structopt(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}
