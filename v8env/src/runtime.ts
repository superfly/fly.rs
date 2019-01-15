export interface Runtime {
  build: string;
}

// injected at build time
export const runtime: Runtime = {
  build: "" 
};