use crate::errors::*;

use std::path::{Path, PathBuf};

use std::marker::{ Send };

use crate::utils::{ take_last_n };

pub trait ModuleResolver: Send {
  fn resolve_module(
    &self, 
    module_specifier: &str,
    containing_file: &str,
  ) -> FlyResult<ModuleInfo>;
}

pub struct ModuleInfo {
  pub module_id: String,
  pub file_name: String,
  pub source_code: String,
}

pub struct LocalDiskModuleResolver {
  pub root: PathBuf,
}

impl LocalDiskModuleResolver {
  pub fn new(root: Option<&Path>) -> Self {
    let root = match root {
      None => std::env::current_dir().expect("invalid current directory"),
      Some(path) => path.to_path_buf(),
    };

    Self { root }
  }
}

impl ModuleResolver for LocalDiskModuleResolver {
  fn resolve_module(
    &self,
    module_specifier: &str,
    containing_file: &str,
  ) -> FlyResult<ModuleInfo> {
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
      let source_code = std::fs::read_to_string(&module_id.to_str().unwrap().to_string())?;
      return Ok(ModuleInfo {
        module_id: module_id.to_str().unwrap().to_string(),
        file_name: module_id
          .canonicalize()
          .unwrap()
          .to_str()
          .unwrap()
          .to_owned(),
        source_code: source_code,
      });
    }
    let did_set = module_id.set_extension("ts");
    info!("trying module {} ({})", module_id.display(), did_set);
    if module_id.is_file() {
      let source_code = std::fs::read_to_string(&module_id.to_str().unwrap().to_string())?;
      return Ok(ModuleInfo {
        module_id: module_id.to_str().unwrap().to_string(),
        file_name: module_id
          .canonicalize()
          .unwrap()
          .to_str()
          .unwrap()
          .to_owned(),
        source_code: source_code,
      });
    }
    let did_set = module_id.set_extension("js");
    info!("trying module {} ({})", module_id.display(), did_set);
    if module_id.is_file() {
      let source_code = std::fs::read_to_string(&module_id.to_str().unwrap().to_string())?;
      return Ok(ModuleInfo {
        module_id: module_id.to_str().unwrap().to_string(),
        file_name: module_id
          .canonicalize()
          .unwrap()
          .to_str()
          .unwrap()
          .to_owned(),
        source_code: source_code,
      });
    }
    // TODO: Add code here for json files and other media types.
    error!("NOPE");

    Err(FlyError::from(format!(
      "Could not resolve {} from {}",
      module_specifier, containing_file
    )))
  }
}

pub struct FunctionModuleResolver {
  resolve_fn: Box<Fn(&str, &str) -> FlyResult<ModuleInfo> + Send>,
}

impl FunctionModuleResolver {
  pub fn new(resolve_fn: Box<Fn(&str, &str) -> FlyResult<ModuleInfo> + Send>) -> Self {
    Self { resolve_fn }
  }
}

impl ModuleResolver for FunctionModuleResolver {
  fn resolve_module(
    &self,
    module_specifier: &str,
    containing_file: &str,
  ) -> FlyResult<ModuleInfo> {
    println!(
      "resolve_module {} from {}",
      module_specifier, containing_file
    );
    (self.resolve_fn)(module_specifier, containing_file)
  }
}

pub struct JsonSecretsResolver {
  base_alias: String,
  json_value: serde_json::Value,
}

impl JsonSecretsResolver {
  pub fn new(base_alias: String, json_value: serde_json::Value) -> Self {
    Self { base_alias, json_value }
  }
}

impl ModuleResolver for JsonSecretsResolver {
  fn resolve_module(
    &self,
    module_specifier: &str,
    containing_file: &str,
  ) -> FlyResult<ModuleInfo> {
    info!("Checking for match of {} on {}", &self.base_alias, module_specifier);
    if module_specifier.starts_with(&self.base_alias) {
      match take_last_n(module_specifier, module_specifier.len() - &self.base_alias.len()) {
        Some(path) => {
          info!("Path resolved to {}", path);
          return Ok(ModuleInfo {
            module_id: format!("{}{}", module_specifier.to_string(), ".json"),
            file_name: format!("{}{}", module_specifier.to_string(), ".json"),
            source_code: self.json_value.to_string(),
          });
        },
        None => {
          return Err(FlyError::from(format!(
            "Could not resolve {} from {}",
            module_specifier, containing_file
          )));
        }
      }
    } else {
      return Err(FlyError::from(format!(
        "Could not resolve {} from {}",
        module_specifier, containing_file
      )));
    }
  }
}

pub struct Compiler {
  pub module_resolvers: Vec<Box<ModuleResolver>>,
}

impl Compiler {
  #[allow(dead_code)]
  pub fn new(module_resolvers: Vec<Box<ModuleResolver>>) -> Self {
    Self { module_resolvers }
  }

  pub fn fetch_module(
    &self,
    module_specifier: &str,
    containing_file: &str,
  ) -> FlyResult<ModuleInfo> {
    info!(
      "fetch_module {} from {}",
      &module_specifier, &containing_file
    );
    for resolver in &self.module_resolvers {
      match resolver.resolve_module(module_specifier, containing_file) {
        Ok(m) => return Ok(m),
        Err(_err) => info!("resolver failed moving on"),
      };
    }
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
    // TODO: these module ids should be normalized into a URL:
    // https://html.spec.whatwg.org/multipage/webappapis.html#resolve-a-module-specifier
    let cases = [
      (
        "./tests/hello.ts",
        ".",
        "././tests/hello.ts",
        "<cwd>/tests/hello.ts",
      ),
      (
        "./hello.ts",
        "./tests/main.ts",
        "./tests/./hello.ts",
        "<cwd>/tests/hello.ts",
      ),
      (
        "../hello.ts",
        "./tests/subdir/index.ts",
        "./tests/subdir/../hello.ts",
        "<cwd>/tests/hello.ts",
      ),
      (
        "<cwd>/tests/hello.ts",
        ".",
        "<cwd>/tests/hello.ts",
        "<cwd>/tests/hello.ts",
      ),
    ];
    let current_dir = std::env::current_dir().expect("current_dir failed");
    let local_disk_resolver = LocalDiskModuleResolver::new(None);
    let resolvers = vec![Box::new(local_disk_resolver) as Box<ModuleResolver>];
    let compiler = Compiler::new(resolvers);

    for &test in cases.iter() {
      let specifier = String::from(test.0).replace("<cwd>", current_dir.to_str().unwrap());
      let containing_file = String::from(test.1).replace("<cwd>", current_dir.to_str().unwrap());
      ;
      let module_info = compiler
        .fetch_module(&specifier, &containing_file)
        .unwrap();
      assert_eq!(
        String::from(test.2).replace("<cwd>", current_dir.to_str().unwrap()),
        module_info.module_id,
      );
      assert_eq!(
        String::from(test.3).replace("<cwd>", current_dir.to_str().unwrap()),
        module_info.file_name,
      );
    }
  }
}
