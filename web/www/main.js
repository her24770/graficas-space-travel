import init, { App } from "./pkg/web.js";

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");

const wasm = await init();
const app = new App(canvas.width, canvas.height);

let last = performance.now();
function loop(now) {
  const dt = (now - last) / 1000;
  last = now;
  app.frame(dt);
  const bytes = new Uint8ClampedArray(wasm.memory.buffer, app.pixels_ptr(), app.pixels_len());
  ctx.putImageData(new ImageData(bytes, canvas.width, canvas.height), 0, 0);
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);
