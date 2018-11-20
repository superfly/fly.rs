import { fullName } from "./fullName"
import { greet } from "./greet"

const firstName: string = "Michael"
const lastName: string = "Dwan"

const name = fullName(firstName, lastName);
console.log(name)

const greeting = greet(firstName);
console.log(greeting)

// uncomment this and you'll get an error
// console.log(fullName("first"));

// uncomment this and you'll get another error!
// console.log(greet(123));