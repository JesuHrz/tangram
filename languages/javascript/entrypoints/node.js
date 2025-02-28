import { setNative } from "./common.js"
import * as os from "os"

export * from "./common.js"

import { createRequire } from "module"
let require = createRequire(import.meta.url)

if (!globalThis.fetch) {
	globalThis.fetch = require("node-fetch")
}
let target = null
let arch = os.arch()
let platform = os.platform()
if (arch === "x64" && platform === "linux") {
	target = "x86_64-linux-gnu"
} else if (arch === "arm64" && platform === "linux") {
	target = "aarch64-linux-gnu"
} else if (arch === "x64" && platform === "darwin") {
	target = "x86_64-macos"
} else if (arch === "arm64" && platform === "darwin") {
	target = "aarch64-macos"
} else if (arch === "x64" && platform === "win32") {
	target = "x86_64-windows-msvc"
}

if (target !== null) {
	setNative(require(`./tangram_${target}.node`))
} else {
	setNative(require("./tangram_wasm.cjs"))
}
