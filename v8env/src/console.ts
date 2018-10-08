// tslint:disable-next-line:no-any
type ConsoleContext = Set<any>;

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

// tslint:disable-next-line:no-any
function stringify(ctx: ConsoleContext, value: any): string {
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

// Print strings when they are inside of arrays or objects with quotes
// tslint:disable-next-line:no-any
function stringifyWithQuotes(ctx: ConsoleContext, value: any): string {
	switch (typeof value) {
		case "string":
			return `"${value}"`;
		default:
			return stringify(ctx, value);
	}
}

// tslint:disable-next-line:no-any
export function stringifyArgs(args: any[]): string {
	const out: string[] = [];
	for (const a of args) {
		if (typeof a === "string") {
			out.push(a);
		} else {
			// tslint:disable-next-line:no-any
			out.push(stringify(new Set<any>(), a));
		}
	}
	return out.join(" ");
}

type PrintFunc = (level: number, msg: string) => void;

const LogLevelError = 0
const LogLevelWarn = 1
const LogLevelInfo = 2
const LogLevelDebug = 3
const LogLevelTrace = 4

export class Console {
	constructor(private printFunc: PrintFunc) { }

	public error(...args: any[]): void {
		this.printFunc(LogLevelError, stringifyArgs(args))
	}

	public warn(...args: any[]): void {
		this.printFunc(LogLevelWarn, stringifyArgs(args))
	}

	public info(...args: any[]): void {
		this.printFunc(LogLevelInfo, stringifyArgs(args))
	}

	public debug(...args: any[]): void {
		this.printFunc(LogLevelDebug, stringifyArgs(args))
	}

	public trace(...args: any[]): void {
		this.printFunc(LogLevelTrace, stringifyArgs(args))
	}

	public log = this.info;

	// tslint:disable-next-line:no-any
	public assert(condition: boolean, ...args: any[]): void {
		if (!condition) {
			this.error(`Assertion failed:`, args)
		}
	}
}