
export function stringifyTypeName(value: any): string {
  if (value === undefined) {
    return 'undefined';
  } else if (value === null) {
    return 'null';
  // } else if (Buffer.isBuffer(value)) {
  //   return 'buffer';
  }
  return Object.prototype.toString
    .call(value)
    .replace(/^\[.+\s(.+?)]$/, '$1')
    .toLowerCase();
}
