declare module 'http-cache-semantics' {
  export class CachePolicy {
    _status: any

    static fromObject(obj: any): CachePolicy
    constructor(obj: any, res: any)
    satisfiesWithoutRevalidation(req: any): boolean
    responseHeaders(): any
    timeToLive(): number
    storable(): boolean
    toObject(): any
  }
  export default CachePolicy
}