use crate::errors::FlyCliResult;
use clap::{AppSettings, ArgMatches, SubCommand};
use globset::{Glob, GlobSetBuilder};
use walkdir::{DirEntry, WalkDir};

pub type App = clap::App<'static, 'static>;

pub type ExecFn = fn(&ArgMatches<'_>) -> FlyCliResult<()>;

pub fn subcommand(name: &'static str) -> App {
    SubCommand::with_name(name).settings(&[
        AppSettings::UnifiedHelpMessage,
        AppSettings::DeriveDisplayOrder,
        AppSettings::DontCollapseArgsInUsage,
    ])
}

pub fn glob(patterns: Vec<&str>) -> FlyCliResult<Vec<String>> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }

    let glob = builder.build()?;

    let mut files: Vec<String> = vec![];

    for entry in WalkDir::new(".")
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| !skip(e))
        .filter_map(|e| e.ok())
    {
        if !glob.is_match(entry.path()) {
            continue;
        }

        if let Some(path) = entry.path().to_str() {
            let path = path.to_owned();
            if !files.contains(&path) {
                files.push(path.to_owned());
            }
        }
    }

    Ok(files)
}

fn skip(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with(".") || s == "node_modules")
        .unwrap_or(false)
}
