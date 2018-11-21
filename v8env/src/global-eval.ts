export const globalEval = eval;

/**
 * Evaluates and executes JavaScript code in the fly
 * runtime global context
 */
export type GlobalEval = (s: string) => any;