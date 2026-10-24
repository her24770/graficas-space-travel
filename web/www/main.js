import init, { App } from "./pkg/web.js";

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");

// Mismos bits que engine::input::bits
const TECLAS = {
  KeyW: 1, KeyS: 2, KeyA: 4, KeyD: 8,
  ArrowUp: 16, ArrowDown: 32, Space: 64,
};
const CICLAR_MODELO = 128; // Tab
let presionadas = 0;
let unaVez = 0;

addEventListener("keydown", (e) => {
  if (e.code === "Tab") { if (!e.repeat) unaVez |= CICLAR_MODELO; e.preventDefault(); return; }
  if (e.code in TECLAS) { presionadas |= TECLAS[e.code]; e.preventDefault(); }
});
addEventListener("keyup", (e) => {
  if (e.code in TECLAS) { presionadas &= ~TECLAS[e.code]; e.preventDefault(); }
});
addEventListener("blur", () => { presionadas = 0; });

const wasm = await init();
const app = new App(canvas.width, canvas.height);

// Multijugador: JS solo mueve bytes; el protocolo está en Rust.
// Si no hay servidor, se juega solo y se reintenta cada 2 s.
let socket = null;
function conectar() {
  const proto = location.protocol === "https:" ? "wss" : "ws";
  const ws = new WebSocket(`${proto}://${location.host}/ws`);
  ws.binaryType = "arraybuffer";
  ws.onopen = () => { socket = ws; };
  ws.onmessage = (e) => app.net_message(new Uint8Array(e.data));
  ws.onclose = () => { socket = null; app.net_closed(); setTimeout(conectar, 2000); };
}
conectar();

let last = performance.now();
let n = 0;
function loop(now) {
  const dt = (now - last) / 1000;
  last = now;
  app.set_input(presionadas | unaVez);
  unaVez = 0;
  app.frame(dt);
  const salida = app.take_outgoing();
  if (salida.length && socket && socket.readyState === WebSocket.OPEN) socket.send(salida);
  const bytes = new Uint8ClampedArray(wasm.memory.buffer, app.pixels_ptr(), app.pixels_len());
  ctx.putImageData(new ImageData(bytes, canvas.width, canvas.height), 0, 0);
  if (++n % 15 === 0) document.title = app.status();
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);
