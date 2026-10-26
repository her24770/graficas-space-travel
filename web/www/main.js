import init, { App } from "./pkg/web.js";

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");

// Se dibuja con una altura fija y el ancho sigue la proporción de la ventana: así
// la pantalla completa se ve nítida y el renderer de software no se vuelve lento.
const ALTO = 540;
function medidas() {
  const ancho = Math.round((ALTO * innerWidth) / Math.max(innerHeight, 1) / 2) * 2;
  return [Math.min(Math.max(ancho, 320), 2400), ALTO];
}

// Mismos bits que engine::input::bits
const TECLAS = {
  KeyW: 1, KeyS: 2, KeyA: 4, KeyD: 8,
  ArrowUp: 16, ArrowDown: 32, Space: 64,
};
const CICLAR_MODELO = 128; // Tab
const EMPEZAR = 256; // Enter
let presionadas = 0;
let unaVez = 0;

function pantallaCompleta() {
  if (document.fullscreenElement) document.exitFullscreen();
  else document.documentElement.requestFullscreen?.();
}

addEventListener("keydown", (e) => {
  if (e.code === "Tab") { if (!e.repeat) unaVez |= CICLAR_MODELO; e.preventDefault(); return; }
  if (e.code === "Enter") { if (!e.repeat) unaVez |= EMPEZAR; e.preventDefault(); return; }
  if (e.code === "KeyF") { if (!e.repeat) pantallaCompleta(); e.preventDefault(); return; }
  if (e.code in TECLAS) { presionadas |= TECLAS[e.code]; e.preventDefault(); }
});
addEventListener("keyup", (e) => {
  if (e.code in TECLAS) { presionadas &= ~TECLAS[e.code]; e.preventDefault(); }
});
addEventListener("blur", () => { presionadas = 0; });
canvas.addEventListener("dblclick", pantallaCompleta);

const wasm = await init();
let [ancho, alto] = medidas();
canvas.width = ancho;
canvas.height = alto;
const app = new App(ancho, alto);

addEventListener("resize", () => {
  [ancho, alto] = medidas();
  canvas.width = ancho;
  canvas.height = alto;
  app.resize(ancho, alto);
});

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
  for (let m = app.take_outgoing(); m.length; m = app.take_outgoing()) {
    if (socket && socket.readyState === WebSocket.OPEN) socket.send(m);
  }
  const bytes = new Uint8ClampedArray(wasm.memory.buffer, app.pixels_ptr(), app.pixels_len());
  ctx.putImageData(new ImageData(bytes, ancho, alto), 0, 0);
  if (++n % 15 === 0) document.title = app.status();
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);
