
export const files: { [name: string]: string } = {
  "fullName.ts": "export function fullName(firstName: string, lastName: string): string {\n  return `${firstName} ${lastName}`;\n}\n",
  "greet.ts": "export function greet(name: string): string {\n  return `Hello, ${name}`;\n}",
  "index.ts": "import { fullName } from \"./fullName\"\nimport { greet } from \"./greet\"\n\nconst firstName: string = \"Michael\"\nconst lastName: string = \"Dwan\"\n\nconst name = fullName(firstName, lastName);\nconsole.log(name)\n\nconst greeting = greet(firstName);\nconsole.log(greeting)\n\n// uncomment this and you'll get an error\n// console.log(fullName(\"first\"));\n\n// uncomment this and you'll get another error!\n// console.log(greet(123));"
};
