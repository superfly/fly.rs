// Copyright 2018 the Deno authors. All rights reserved. MIT license.
import * as domTypes from "./dom_types";
import * as blob from "./blob";
import { DomIterableMixin } from "./mixins/dom_iterable";
import { stringify } from "querystring"

const dataSymbol = Symbol("data");

class FormDataBase {
  private [dataSymbol]: Array<[string, domTypes.FormDataEntryValue]> = [];

  /** Appends a new value onto an existing key inside a `FormData`
   * object, or adds the key if it does not already exist.
   *
   *       formData.append('name', 'first');
   *       formData.append('name', 'second');
   */
  append(name: string, value: string): void {
    // append(name: string, value: blob.FlyBlob, filename?: string): void;
    // append(name: string, value: string | blob.FlyBlob, filename?: string): void {
    // if (value instanceof blob.FlyBlob) {
    //   const dfile = new file.DenoFile([value], filename || name);
    //   this[dataSymbol].push([name, dfile]);
    // } else {
    this[dataSymbol].push([name, String(value)]);
    // }
  }

  /** Deletes a key/value pair from a `FormData` object.
   *
   *       formData.delete('name');
   */
  delete(name: string): void {
    let i = 0;
    while (i < this[dataSymbol].length) {
      if (this[dataSymbol][i][0] === name) {
        this[dataSymbol].splice(i, 1);
      } else {
        i++;
      }
    }
  }

  /** Returns an array of all the values associated with a given key
   * from within a `FormData`.
   *
   *       formData.getAll('name');
   */
  getAll(name: string): domTypes.FormDataEntryValue[] {
    const values = [];
    for (const entry of this[dataSymbol]) {
      if (entry[0] === name) {
        values.push(entry[1]);
      }
    }

    return values;
  }

  /** Returns the first value associated with a given key from within a
   * `FormData` object.
   *
   *       formData.get('name');
   */
  get(name: string): domTypes.FormDataEntryValue | null {
    for (const entry of this[dataSymbol]) {
      if (entry[0] === name) {
        return entry[1];
      }
    }

    return null;
  }

  /** Returns a boolean stating whether a `FormData` object contains a
   * certain key/value pair.
   *
   *       formData.has('name');
   */
  has(name: string): boolean {
    return this[dataSymbol].some(entry => entry[0] === name);
  }

  /** Sets a new value for an existing key inside a `FormData` object, or
   * adds the key/value if it does not already exist.
   *
   *       formData.set('name', 'value');
   */
  set(name: string, value: string): void {
    // set(name: string, value: blob.FlyBlob, filename?: string): void;
    // set(name: string, value: string | blob.FlyBlob, filename?: string): void {
    this.delete(name);
    // if (value instanceof blob.FlyBlob) {
    //   const dfile = new file.DenoFile([value], filename || name);
    //   this[dataSymbol].push([name, dfile]);
    // } else {
    this[dataSymbol].push([name, String(value)]);
    // }
  }

  public toString(): string {
    return stringify(this[dataSymbol].reduce((acc, [name, value]) => {
      let found = acc[name];
      if (typeof found === 'undefined')
        acc[name] = value
      else if (Array.isArray(found))
        acc[name].push(value)
      else
        acc[name] = [found, value]
      return acc
    }, {}))
  }
}

// tslint:disable-next-line:variable-name
export class FlyFormData extends DomIterableMixin<
  string,
  domTypes.FormDataEntryValue,
  typeof FormDataBase
>(FormDataBase, dataSymbol) { };