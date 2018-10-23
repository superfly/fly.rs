import path from "path-browserify"

export type ModuleSpecifier = string
export type ModuleId = string
export type ContainingFile = string

export async function resolveModule(id: ModuleId, containingFile?: ContainingFile): Promise<string | undefined> {
  console.log("resolveModule", { id, containingFile })

  if (isAbsolute(id)) {
    // module is an absolute path, pass through
    return id
  }

  if (id.startsWith(".")) {
    // module is a relative reference, resolve
    let filename: string = "";
    if (containingFile) {
      const parentDir = path.dirname(containingFile);
      console.log("resolve", { containingFile, parentDir, filename })
      filename = path.resolve("/", parentDir, id)
      console.log("resolve", { containingFile, parentDir, filename })
    } else {
      filename = path.resolve("/", id)
      console.log("resolve", { containingFile, filename })
    }

    if (filename.startsWith("/")) {
      filename = filename.substring(1)
    }

    if (await exists(filename)) {
      return filename
    }
    // no extension specified, try ts, js, etc
    if (path.extname(filename) === "") {
      for (const ext of ["ts", "js"]) {
        const filenameWithExt = `${filename}.${ext}`;
        if (await exists(filenameWithExt)) {
          return filenameWithExt
        }
      }
    }

    // no file found
    // console.log("no file found")
    return null
  }

  if (await exists(id)) {
    return id
  }

  throw new Error("resolution failed")

  // module is external, resolve against node modules or something
  return `external/${id}`
}

export async function loadModule(id: ModuleId): Promise<string | undefined> {
  console.log("loadModule", { id })

  const resp = await fetch(`file://${id}`)
  const source = await resp.text();
  console.log("got source", source.length, source)
  return source.substring(0)

  // try {
  //   const builder = new flatbuffers.Builder();
  //   const id_fb = builder.createString(id);
  //   fbs.ResolveModuleRequest.startResolveModuleRequest(builder);
  //   fbs.ResolveModuleRequest.addModuleSpecifier(builder, id_fb);
  //   const msg = fbs.ResolveModuleRequest.endResolveModuleRequest(builder);
  //   const resp = sendSync(builder, fbs.Any.ResolveModuleRequest, msg);
  //   console.log("loadModule resp", resp);
  //   const respMsg = new fbs.ResolveModuleResponse();
  //   resp.msg(respMsg);
  //   const sourceCode = respMsg.sourceCode()
  //   console.log("got resolve resp", { sourceCode })
  //   return Promise.resolve(sourceCode)
  // } catch (e) {
  //   console.error("error from rust", e)
  //   return Promise.reject(e)
  // }
}

const absolutePathPattern = /^(?:\/|(?:[A-Za-z]:)?[\\|/])/;

function isAbsolute(modulePath: string) {
  return absolutePathPattern.test(modulePath);
}

export async function exists(path: string): Promise<boolean> {
  console.log("exists", { path, uri: `file://${path}` })
  try {
    // should this error or 404?
    const resp = await fetch(`file://${path}`)
    console.log("exists resp", { ok: resp.ok })
    return resp.ok;
  } catch (error) {
    console.warn(`doesn't exist`, { path })
    return false
  }
}