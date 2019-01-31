use crate::errors::*;

use std::path::PathBuf;

use std::marker::Send;

use std::clone::Clone;

use std::collections::HashMap;

use serde_json;

#[derive(Clone, Debug)]
pub struct RefererInfo {
    pub origin_url: String,
    pub is_wasm: Option<bool>,
    pub source_code: Option<String>,
    pub indentifier_hash: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct LoadedSourceCode {
    pub is_wasm: bool,
    pub source_map: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct LoadedModule {
    pub loaded_source: LoadedSourceCode,
    pub origin_url: String,
}

pub struct ModuleSourceData {
    pub origin_url: String,
    pub source_loader: Box<SourceLoader>,
}

/**
 * Similar function to what is known as a "loader" in the javascript packaging world
 */
pub trait SourceLoader: Send {
    fn load_source(&self) -> FlyResult<LoadedSourceCode>;
}

/**
 * Resolves a module specifier and returns a "strategy" for loading the module to ES6 or WASM code.
 */
pub trait ModuleResolver: Send + Sync {
    fn resolve_module(
        &self,
        module_specifier: &str,
        referer_info: Option<RefererInfo>,
    ) -> FlyResult<ModuleSourceData>;
    fn get_protocol(&self) -> String;
}

/**
 * This trait is a used as the "front door" of the dynamic module resolution system.
 */
pub trait ModuleResolverManager: Send + Sync {
    fn resolve_module(
        &self,
        specifier: String,
        referer_info: Option<RefererInfo>,
    ) -> FlyResult<LoadedModule>;
}

/**
 * Parse url or join it to the working url if it's relative. working_url_str << MUST BE AN ABSOLUTE PATH.
 */
fn parse_url(url_str: &str, working_url_str: &str) -> Result<url::Url, url::ParseError> {
    debug!("parse_url {} from {}", &url_str, &working_url_str);
    // TODO: Add some additional logic to this thing to account for file paths without the "file://" protocol denotation.
    let mut url_parsed = match url::Url::parse(url_str) {
        Ok(v) => v,
        Err(e) => {
            if e == url::ParseError::RelativeUrlWithoutBase {
                // If the url is relative join it to the working path.
                trace!("Url relative: {}", url_str);
                let working_url_parsed = url::Url::parse(working_url_str)?;
                let final_url = working_url_parsed.join(url_str)?;
                final_url
            } else {
                return Err(e);
            }
        }
    };

    // The default scheme/protocol should be "file://"
    if url_parsed.scheme() == "" {
        // ignore result since "file" scheme won't error
        let _ = url_parsed.set_scheme("file");
    }

    return Ok(url_parsed);
}

pub struct LocalDiskRawLoader {
    pub source_file_path: PathBuf,
    pub source_map_path: Option<PathBuf>,
}

impl LocalDiskRawLoader {
    pub fn new(source_file_path: PathBuf, source_map_path: Option<PathBuf>) -> Self {
        Self {
            source_file_path,
            source_map_path,
        }
    }
}

impl SourceLoader for LocalDiskRawLoader {
    fn load_source(&self) -> FlyResult<LoadedSourceCode> {
        // Try to load file from path for this loader and return if successful
        let source = std::fs::read_to_string(&self.source_file_path.to_str().unwrap().to_string())?;
        let source_map = match &self.source_map_path {
            Some(v) => match std::fs::read_to_string(&v.to_str().unwrap().to_string()) {
                Ok(v) => Some(v),
                Err(_err) => None,
            },
            None => None,
        };
        Ok(LoadedSourceCode {
            is_wasm: false,
            source_map,
            source,
        })
    }
}

pub struct LocalDiskModuleResolver {
    pub default_working_url: String,
}

impl LocalDiskModuleResolver {
    pub fn new(default_working_url: Option<String>) -> Self {
        let default_working_url = match default_working_url {
            None => url::Url::from_directory_path(
                std::env::current_dir().expect("invalid current directory"),
            )
            .unwrap()
            .as_str()
            .to_string(),
            Some(default_working_url) => default_working_url,
        };
        Self {
            default_working_url,
        }
    }
}

impl ModuleResolver for LocalDiskModuleResolver {
    fn resolve_module(
        &self,
        module_specifier: &str,
        referer_info: Option<RefererInfo>,
    ) -> FlyResult<ModuleSourceData> {
        let referer_origin_url = match referer_info {
            Some(v) => v.origin_url,
            None => self.default_working_url.clone(),
        };
        trace!(
            "resolve_module {} from {}",
            module_specifier,
            referer_origin_url
        );

        let module_specifier_url = parse_url(module_specifier, referer_origin_url.as_str())?;

        let mut module_file_path = module_specifier_url.to_file_path()?;

        if module_file_path.is_file() {
            return Ok(ModuleSourceData {
                origin_url: url::Url::from_file_path(module_file_path.clone())
                    .unwrap()
                    .as_str()
                    .to_string(),
                source_loader: Box::new(LocalDiskRawLoader::new(module_file_path, None)),
            });
        }
        let did_set = module_file_path.set_extension("ts");
        trace!("trying module {} ({})", module_file_path.display(), did_set);
        if module_file_path.is_file() {
            return Ok(ModuleSourceData {
                origin_url: url::Url::from_file_path(module_file_path.clone())
                    .unwrap()
                    .as_str()
                    .to_string(),
                source_loader: Box::new(LocalDiskRawLoader::new(module_file_path, None)),
            });
        }
        let did_set = module_file_path.set_extension("js");
        trace!("trying module {} ({})", module_file_path.display(), did_set);
        if module_file_path.is_file() {
            return Ok(ModuleSourceData {
                origin_url: url::Url::from_file_path(module_file_path.clone())
                    .unwrap()
                    .as_str()
                    .to_string(),
                source_loader: Box::new(LocalDiskRawLoader::new(module_file_path, None)),
            });
        }
        // TODO: Add code here for json files and other media types.
        error!("NOPE");

        Err(FlyError::from(format!(
            "Could not resolve {} from {} ",
            module_specifier, referer_origin_url
        )))
    }
    fn get_protocol(&self) -> String {
        return "file".to_string();
    }
}

pub struct FunctionModuleResolver {
    resolve_fn: Box<Fn(&str, Option<RefererInfo>) -> FlyResult<ModuleSourceData> + Send + Sync>,
}

impl FunctionModuleResolver {
    pub fn new(
        resolve_fn: Box<Fn(&str, Option<RefererInfo>) -> FlyResult<ModuleSourceData> + Send + Sync>,
    ) -> Self {
        Self { resolve_fn }
    }
}

impl ModuleResolver for FunctionModuleResolver {
    fn resolve_module(
        &self,
        module_specifier: &str,
        referer_info: Option<RefererInfo>,
    ) -> FlyResult<ModuleSourceData> {
        let referer_origin_url = match referer_info.clone() {
            Some(v) => v.origin_url,
            None => "".to_string(),
        };
        trace!(
            "resolve_module {} from {}",
            module_specifier,
            referer_origin_url
        );
        (self.resolve_fn)(module_specifier, referer_info)
    }
    fn get_protocol(&self) -> String {
        return "function".to_string();
    }
}

pub struct JsonSecretsLoader {
    pub json_value: serde_json::Value,
}

impl JsonSecretsLoader {
    pub fn new(json_value: &serde_json::Value) -> Self {
        Self {
            json_value: (*json_value).clone(),
        }
    }
}

impl SourceLoader for JsonSecretsLoader {
    fn load_source(&self) -> FlyResult<LoadedSourceCode> {
        let source_code = format!(
            "export default JSON.parse(`{}`);",
            self.json_value.to_string().replace("`", "")
        );

        trace!("Loaded json secrets {}", source_code);

        return Ok(LoadedSourceCode {
            is_wasm: false,
            source_map: None,
            source: source_code,
        });
    }
}

pub struct JsonSecretsResolver {
    json_value: serde_json::Value,
}

impl JsonSecretsResolver {
    pub fn new(json_value: serde_json::Value) -> Self {
        Self { json_value }
    }
}

impl ModuleResolver for JsonSecretsResolver {
    fn resolve_module(
        &self,
        module_specifier: &str,
        referer_info: Option<RefererInfo>,
    ) -> FlyResult<ModuleSourceData> {
        let referer_origin_url = match referer_info {
            Some(v) => v.origin_url,
            None => "secrets:///".to_string(),
        };
        let module_specifier_url = parse_url(module_specifier, referer_origin_url.as_str())?;
        // TODO: add some origin checks for referer.
        let mut value = self.json_value.clone();
        let path_segs = module_specifier_url.path_segments().unwrap();
        for path_seg in path_segs {
            if value[path_seg] != serde_json::Value::Null {
                value = value[path_seg].clone();
            } else {
                return Err(FlyError::from(format!(
                    "Could not resolve {} from {} ",
                    module_specifier_url, referer_origin_url
                )));
            }
        }
        return Ok(ModuleSourceData {
            origin_url: format!("{}{}", module_specifier.to_string(), ".js"),
            source_loader: Box::new(JsonSecretsLoader::new(&value)),
        });
    }
    fn get_protocol(&self) -> String {
        return "secrets".to_string();
    }
}

pub struct StandardModuleResolverManager {
    protocol_resolver_map: HashMap<String, Vec<Box<ModuleResolver>>>,
    default_working_url: String,
}

impl StandardModuleResolverManager {
    pub fn new(resolvers: Vec<Box<ModuleResolver>>, default_working_url: Option<String>) -> Self {
        // Create protocol to resolver map and map out resolvers.
        let mut protocol_resolver_map: HashMap<String, Vec<Box<ModuleResolver>>> = HashMap::new();
        for resolver in resolvers {
            match protocol_resolver_map.get_mut(&resolver.get_protocol()) {
                Some(v) => v.push(resolver),
                None => {
                    protocol_resolver_map.insert(resolver.get_protocol(), vec![resolver]);
                }
            }
        }
        let default_working_url = match default_working_url {
            None => url::Url::from_directory_path(
                std::env::current_dir().expect("invalid current directory"),
            )
            .unwrap()
            .as_str()
            .to_string(),
            Some(default_working_url) => default_working_url,
        };
        Self {
            protocol_resolver_map,
            default_working_url,
        }
    }
}

impl ModuleResolverManager for StandardModuleResolverManager {
    fn resolve_module(
        &self,
        specifier: String,
        referer_info: Option<RefererInfo>,
    ) -> FlyResult<LoadedModule> {
        let referer_origin_url = match referer_info.clone() {
            Some(v) => v.origin_url,
            None => self.default_working_url.clone(),
        };
        // Parse the specifier with the referer origin_url as the working path/url.
        info!("resolve_module {} from {}", &specifier, &referer_origin_url);

        let specifier_url = parse_url(specifier.as_str(), referer_origin_url.as_str())?;

        // Try to get a vector of the resolvers for the protocol we are tring to resolve.
        let resolvers = match self.protocol_resolver_map.get(specifier_url.scheme()) {
            Some(v) => v,
            None => {
                return Err(FlyError::from(format!(
                    "Could not resolve {} from {}: no resolvers for protocol {} setup.",
                    specifier,
                    &referer_origin_url,
                    specifier_url.scheme()
                )));
            }
        };

        for resolver in resolvers {
            let resolver_result = resolver.resolve_module(specifier.as_str(), referer_info.clone());
            if let Err(e) = resolver_result {
                info!("Resolver failed trying the next one: {}", e);
            } else {
                let module_loader = resolver_result.unwrap();
                let loaded_source = module_loader.source_loader.load_source()?;
                return Ok(LoadedModule {
                    loaded_source,
                    origin_url: module_loader.origin_url,
                });
            }
        }

        Err(FlyError::from(format!(
            "Could not resolve {} from {}: exausted all resolvers.",
            specifier, referer_origin_url
        )))
    }
}
