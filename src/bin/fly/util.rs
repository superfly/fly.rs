use crate::errors::{FlyCliError, FlyCliResult};
use clap::{AppSettings, ArgMatches, SubCommand};
use std::error::Error;

pub type App = clap::App<'static, 'static>;

pub type ExecFn = fn(&ArgMatches<'_>) -> FlyCliResult<()>;

pub fn subcommand(name: &'static str) -> App {
    SubCommand::with_name(name).settings(&[
        AppSettings::UnifiedHelpMessage,
        AppSettings::DeriveDisplayOrder,
        AppSettings::DontCollapseArgsInUsage,
    ])
}

pub fn glob(patterns: Vec<&str>, max_depth: Option<usize>) -> FlyCliResult<Vec<String>> {
    let patterns: Vec<&str> = patterns.into_iter().map(clean_pattern).collect();

    let mut builder = globwalk::GlobWalkerBuilder::from_patterns(".", &patterns);

    if let Some(d) = max_depth {
        builder = builder.max_depth(d);
    }

    let walker = builder
        .build()
        .map_err(|e| FlyCliError::from(e.description()))?;

    let mut files: Vec<String> = vec![];

    for entry in walker {
        if let Ok(entry) = entry {
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            if let Some(path) = path.to_str() {
                files.push(path.to_owned());
            }
        }
    }

    Ok(files)
}

fn clean_pattern(pattern: &str) -> &str {
    if pattern.starts_with("./") {
        return &pattern[2..];
    }
    pattern
}
