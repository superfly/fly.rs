import { fullName } from "./fullName"
import { greet } from "./greet"

const firstName = "Jon";
const lastName = "Phenow";

console.log("This comes from a javascript module module:")
const name = fullName(firstName, lastName);
console.log(name)

console.log("This comes from a typescript module:")
const greeting = greet(firstName);
console.log(greeting)
