import { Logger, Level } from "./logging";

export class Console {
	constructor(private readonly logger: Logger) { }

	public error(...args: any[]): void {
		this.logger.log(Level.AppError, ...args);
	}

	public warn(...args: any[]): void {
		this.logger.log(Level.AppWarn, ...args);
	}

	public info(...args: any[]): void {
		this.logger.log(Level.AppInfo, ...args);
	}

	public debug(...args: any[]): void {
		this.logger.log(Level.AppDebug, ...args);
	}

	public trace(...args: any[]): void {
		this.logger.log(Level.AppTrace, ...args);
	}

	public log = this.info;

	// tslint:disable-next-line:no-any
	public assert(condition: boolean, ...args: any[]): void {
		if (!condition) {
			this.error(`Assertion failed:`, args)
		}
	}
}