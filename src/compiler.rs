use errors::*;

use std::path::{Path, PathBuf};

pub struct Compiler {
  pub root: PathBuf,
}

pub struct ModuleInfo {
  pub module_id: String,
  pub source_code: String,
}

impl Compiler {
  #[allow(dead_code)]
  pub fn new(root: Option<&Path>) -> Self {
    let root = match root {
      None => std::env::current_dir().expect("invalid current directory"),
      Some(path) => path.to_path_buf(),
    };

    Self { root }
  }

  pub fn fetch_module(
    self: &Self,
    module_specifier: &str,
    containing_file: &str,
  ) -> FlyResult<ModuleInfo> {
    info!(
      "fetch_module {} from {}",
      &module_specifier, &containing_file
    );
    let module_id = self.resolve_module(&module_specifier, &containing_file)?;
    info!("resolved {}", &module_id);
    let source_code = std::fs::read_to_string(&module_id)?;
    info!("source_code: {}", &source_code);
    Ok(ModuleInfo {
      module_id: module_id,
      source_code: source_code,
    })
  }

  #[allow(dead_code)]
  pub fn resolve_module(
    self: &Self,
    module_specifier: &str,
    containing_file: &str,
  ) -> FlyResult<String> {
    println!(
      "resolve_module {} from {}",
      module_specifier, containing_file
    );

    let mut base = PathBuf::from(containing_file);
    if base.is_file() {
      base.pop();
    }

    let mut module_id = base.join(module_specifier); //.canonicalize().unwrap();
    info!("trying module {}", module_id.display());
    if module_id.is_file() {
      return Ok(
        module_id
          .canonicalize()
          .unwrap()
          .to_str()
          .unwrap()
          .to_owned(),
      );
    }
    let did_set = module_id.set_extension("ts");
    info!("trying module {} ({})", module_id.display(), did_set);
    if module_id.is_file() {
      return Ok(
        module_id
          .canonicalize()
          .unwrap()
          .to_str()
          .unwrap()
          .to_owned(),
      );
    }
    let did_set = module_id.set_extension("js");
    info!("trying module {} ({})", module_id.display(), did_set);
    if module_id.is_file() {
      return Ok(
        module_id
          .canonicalize()
          .unwrap()
          .to_str()
          .unwrap()
          .to_owned(),
      );
    }
    error!("NOPE");

    Err(FlyError::from(format!(
      "Could not resolve {} from {}",
      module_specifier, containing_file
    )))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_resolve() {
    let cases = [
      ("./tests/hello.ts", ".", "<cwd>/tests/hello.ts"),
      ("./hello.ts", "./tests/main.ts", "<cwd>/tests/hello.ts"),
      (
        "../hello.ts",
        "./tests/subdir/index.ts",
        "<cwd>/tests/hello.ts",
      ),
      ("<cwd>/tests/hello.ts", ".", "<cwd>/tests/hello.ts"),
    ];
    let current_dir = std::env::current_dir().expect("current_dir failed");
    let compiler = Compiler::new(None);

    for &test in cases.iter() {
      let specifier = String::from(test.0).replace("<cwd>", current_dir.to_str().unwrap());
      let containing_file = String::from(test.1).replace("<cwd>", current_dir.to_str().unwrap());
      ;
      let module_id = compiler
        .resolve_module(&specifier, &containing_file)
        .unwrap();
      assert_eq!(
        String::from(test.2).replace("<cwd>", current_dir.to_str().unwrap()),
        module_id,
      );
    }
  }
}
