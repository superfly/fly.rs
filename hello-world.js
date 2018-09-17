console.log("ha");

// console.log(new TextDecoder().decode(new Uint8Array([104, 101, 108, 108, 111])))

let now = Date.now();
setInterval(() => { console.log("in timeout!", Date.now() - now); now = Date.now() }, 500)