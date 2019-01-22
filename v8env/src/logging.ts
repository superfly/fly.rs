type ConsoleContext = Set<unknown>;

type PrintFunc = (level: number, msg: string) => void;

export const enum Level {
  Print = 0,
  RuntimeError = 1,
  RuntimeWarn = 2,
  RuntimeInfo = 3,
  RuntimeDebug = 4,
  RuntimeTrace = 5,
  AppError = 11,
  AppWarn = 12,
  AppInfo = 13,
  AppDebug = 14,
  AppTrace = 15,
}

export class Logger {
  constructor(private printFunc: PrintFunc) { }

  public log(level: Level, ...args: unknown[]) {
    this.printFunc(level, stringifyArgs(args));
  }

  public print(...args: unknown[]): void {
    this.printFunc(Level.Print, stringifyArgs(args));
  }

  public error(...args: unknown[]): void {
    this.printFunc(Level.RuntimeError, stringifyArgs(args))
  }

  public warn(...args: unknown[]): void {
    this.printFunc(Level.RuntimeWarn, stringifyArgs(args))
  }

  public info(...args: unknown[]): void {
    this.printFunc(Level.RuntimeInfo, stringifyArgs(args))
  }

  public debug(...args: unknown[]): void {
    this.printFunc(Level.RuntimeDebug, stringifyArgs(args))
  }

  public trace(...args: unknown[]): void {
    this.printFunc(Level.RuntimeTrace, stringifyArgs(args))
  }
}

// Print strings when they are inside of arrays or objects with quotes
function stringifyWithQuotes(ctx: ConsoleContext, value: unknown): string {
  switch (typeof value) {
    case "string":
      return `"${value}"`;
    default:
      return stringify(ctx, value);
  }
}

function stringifyArgs(args: unknown[]): string {
  const out: string[] = [];
  for (const a of args) {
    if (typeof a === "string") {
      out.push(a);
    } else {
      out.push(stringify(new Set<unknown>(), a));
    }
  }
  return out.join(" ");
}

// tslint:disable-next-line:no-any
function getClassInstanceName(instance: any): string {
  if (typeof instance !== "object") {
    return "";
  }
  if (instance && instance.__proto__ && instance.__proto__.constructor) {
    return instance.__proto__.constructor.name; // could be "Object" or "Array"
  }
  return "";
}

function stringify(ctx: ConsoleContext, value: unknown): string {
  switch (typeof value) {
    case "string":
      return value;
    case "number":
    case "boolean":
    case "undefined":
    case "symbol":
      return String(value);
    case "function":
      if (value.name && value.name !== "anonymous") {
        // from MDN spec
        return `[Function: ${value.name}]`;
      }
      return "[Function]";
    case "object":
      if (value === null) {
        return "null";
      }

      if (ctx.has(value)) {
        return "[Circular]";
      }

      ctx.add(value);
      const entries: string[] = [];

      if (Array.isArray(value)) {
        for (const el of value) {
          entries.push(stringifyWithQuotes(ctx, el));
        }

        ctx.delete(value);

        if (entries.length === 0) {
          return "[]";
        }
        return `[ ${entries.join(", ")} ]`;
      } else {
        let baseString = "";

        const className = getClassInstanceName(value);
        let shouldShowClassName = false;
        if (className && className !== "Object" && className !== "anonymous") {
          shouldShowClassName = true;
        }

        for (const key of Object.keys(value)) {
          entries.push(`${key}: ${stringifyWithQuotes(ctx, value[key])}`);
        }

        ctx.delete(value);

        if (entries.length === 0) {
          baseString = "{}";
        } else {
          baseString = `{ ${entries.join(", ")} }`;
        }

        if (shouldShowClassName) {
          baseString = `${className} ${baseString}`;
        }

        return baseString;
      }
    default:
      return "[Not Implemented]";
  }
}