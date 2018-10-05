import { Headers, HeadersInit } from "./dom_types";
import { assert } from "./util";

interface Header {
	name: string;
	value: string;
}

export type FlyHeadersInit = HeadersInit | FlyHeaders

export class FlyHeaders implements Headers {
	private readonly headerList: Header[] = [];
	length: number

	constructor(init?: FlyHeadersInit) {
		if (init) {
			this._fill(init);
		}
		this.length = this.headerList.length
	}

	private _append(header: Header): void {
		// TODO(qti3e) Check header based on the fetch spec.
		this._appendToHeaderList(header);
	}

	private _appendToHeaderList(header: Header): void {
		const lowerCaseName = header.name.toLowerCase();
		for (let i = 0; i < this.headerList.length; ++i) {
			if (this.headerList[i].name.toLowerCase() === lowerCaseName) {
				header.name = this.headerList[i].name;
			}
		}
		this.headerList.push(header);
	}

	private _fill(init: FlyHeadersInit): void {
		if (init instanceof FlyHeaders) {
			init.forEach((value, name) => {
				this._append({ name: name, value: value })
			})
		} else if (Array.isArray(init)) {
			for (let i = 0; i < init.length; ++i) {
				const header = init[i];
				if (header.length !== 2) {
					throw new TypeError("Failed to construct 'Headers': Invalid value");
				}
				this._append({
					name: header[0],
					value: header[1]
				});
			}
		} else {
			for (const key in init) {
				this._append({
					name: key,
					value: init[key]
				});
			}
		}
	}

	append(name: string, value: string): void {
		this._appendToHeaderList({ name, value });
	}

	delete(name: string): void {
		const idx = this.headerList.findIndex(function (h) {
			return h.name == name.toLowerCase()
		})
		if (idx >= 0)
			this.headerList.splice(idx, 1)
	}
	get(name: string): string | null {
		for (const header of this.headerList) {
			if (header.name.toLowerCase() === name.toLowerCase()) {
				return header.value;
			}
		}
		return null;
	}
	has(name: string): boolean {
		assert(false, "Implement me");
		return false;
	}

	set(name: string, value: string): void {
		assert(false, "Implement me");
	}

	forEach(
		callbackfn: (value: string, key: string, parent: Headers) => void,
		// tslint:disable-next-line:no-any
		thisArg?: any
	): void {
		const it = this[Symbol.iterator]();
		let cur = it.next();
		while (!cur.done) {
			const { name, value } = cur.value;
			callbackfn(value, name, this);
			cur = it.next();
		}
	}

	[Symbol.iterator]() {
		return new FlyHeadersIterator(this.headerList)
	}
}

class FlyHeadersIterator implements Iterator<Header> {
	headers: Header[]
	private index: number
	constructor(headers: Header[]) {
		this.headers = headers;
		this.index = 0;
	}

	next(): IteratorResult<Header> {
		if (this.index >= this.headers.length) { return { value: undefined, done: true } }
		return { value: this.headers[this.index++], done: false }
	}

	[Symbol.iterator]() {
		return this
	}
}