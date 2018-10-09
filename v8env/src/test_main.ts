
import { globalEval } from './global-eval'
// import * as expect from "expect"
// import * as Mocha from "mocha"
// import * as chai from "chai"
// var expect = require('chai').expect

declare var mocha: any

const window = globalEval("this");
if (window.global === undefined) {
  window.global = window
}

window.global.global.location = {}

// window.expect = chai.expect

// console.log("mocha", Mocha)
// console.log("mocha", mocha)
// console.log("window", window)
// console.log("expect", expect)

// const mocha = new Mocha({
//   ui: 'bdd',
//   reporter: 'spec',
//   useColors: true
// })
// mocha.suite.
// window.expect = expect
// window.mocha = mocha
// window.Mocha = mocha

// window.test = Mocha.test
// window.it = Mocha.it
// window.describe = Mocha.describe


// window.runTests = function testMain() {
//   mocha.run()
// }
// export default