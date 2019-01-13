use crate::errors::*;

use crate::module_resolver::{ ModuleResolver };

pub struct ModuleInfo {
  pub module_id: String,
  pub file_name: String,
  pub source_code: String,
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
