// LifeGPU — shared WebGL2 ping-pong + colour-pass harness for the life-*.html sims.
//
// Each sim keeps its own rule: it supplies a simulation fragment shader (one
// generation, reading neighbours from a state texture) and a colour fragment
// shader (state -> RGBA8 for display), plus small callbacks to move its CPU
// arrays in and out of the GPU textures. This module owns the fragile plumbing —
// float textures, framebuffers, the ping-pong swap, the three.js render-target
// borrow, readback and resize — so there is exactly one copy of it to get right.
//
// Browser global only (needs a WebGL2 context); no build step, drop in with
// <script src="life-gpu.js"> alongside three.js. LifeGPU.create() always returns
// an api object; check api.available before relying on the GPU path.
(function (root) {
  'use strict';

  // cfg fields (all sim-specific behaviour lives here):
  //   getSize()            -> [cols, rows]   current logical grid size
  //   simFrag, colorFrag   GLSL ES 3.00 fragment sources
  //   hasEnv               whether a second (env) texture is used
  //   packState(buf,W,H)   fill Float32 RGBA-per-texel buffer from CPU state
  //   unpackState(buf,W,H) write CPU state back from the buffer (readback)
  //   packEnv(buf,W,H)     fill the env texture's R channel (only if hasEnv)
  //   setSimUniforms(u,W,H)   set the sim program's uniforms via helper u
  //   setColorUniforms(u,W,H) set the colour program's uniforms via helper u
  // The harness always binds uState -> unit 0 and uEnv -> unit 1 for both programs.
  function create(renderer, cfg) {
    const api = {
      available: false, enabled: false, dirty: true, envDirty: true,
      colorTexture: null, colorRT: null,
      step() {}, refresh() {}, syncToCPU() {}, resize() {},
      markDirty() { this.dirty = true; }, markEnvDirty() { this.envDirty = true; },
    };
    let gl, ok;
    try {
      gl = renderer.getContext();
      ok = renderer.capabilities.isWebGL2 && gl.getExtension('EXT_color_buffer_float');
    } catch (e) { ok = false; }
    if (!ok) return api;

    try {
      const VERT = `#version 300 es
void main(){ vec2 p = vec2(float((gl_VertexID<<1)&2), float(gl_VertexID&2)); gl_Position = vec4(p*2.0-1.0,0.0,1.0); }`;
      const compile = (t, src) => { const s = gl.createShader(t); gl.shaderSource(s, src); gl.compileShader(s);
        if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s)); return s; };
      const makeProg = (fs) => { const p = gl.createProgram();
        gl.attachShader(p, compile(gl.VERTEX_SHADER, VERT)); gl.attachShader(p, compile(gl.FRAGMENT_SHADER, fs));
        gl.linkProgram(p); if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p)); return p; };
      const prog = makeProg(cfg.simFrag), cprog = makeProg(cfg.colorFrag);

      const locs = new Map(), loc = (pr, n) => { const k = (pr === prog ? 's:' : 'c:') + n;
        if (!locs.has(k)) locs.set(k, gl.getUniformLocation(pr, n)); return locs.get(k); };
      // per-program uniform helper handed to the sim's setUniforms callbacks
      const helper = pr => ({
        i1: (n, v) => gl.uniform1i(loc(pr, n), v),
        f1: (n, v) => gl.uniform1f(loc(pr, n), v),
        f1v: (n, v) => gl.uniform1fv(loc(pr, n), v),
        f3v: (n, v) => gl.uniform3fv(loc(pr, n), v),
        ui1: (n, v) => gl.uniform1ui(loc(pr, n), v),
      });
      const uSim = helper(prog), uColor = helper(cprog);
      const vao = gl.createVertexArray();

      let W = 0, H = 0, texA, texB, texEnv, fboA, fboB, rb = null, colorRT, cfb;
      function mkTex() {
        const t = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, t);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, W, H, 0, gl.RGBA, gl.FLOAT, null);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        return t;
      }
      const mkFbo = t => { const f = gl.createFramebuffer(); gl.bindFramebuffer(gl.FRAMEBUFFER, f);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, t, 0); return f; };
      function makeColorRT() {
        colorRT = new THREE.WebGLRenderTarget(W, H, { depthBuffer: false, stencilBuffer: false });
        colorRT.texture.minFilter = colorRT.texture.magFilter = THREE.NearestFilter;
        colorRT.texture.generateMipmaps = false;
        const prev = renderer.getRenderTarget();      // force three to allocate the GL fbo, then borrow it
        renderer.setRenderTarget(colorRT); renderer.setRenderTarget(prev);
        cfb = renderer.properties.get(colorRT).__webglFramebuffer;
        api.colorTexture = colorRT.texture; api.colorRT = colorRT;
      }
      function alloc() { [W, H] = cfg.getSize(); texA = mkTex(); texB = mkTex(); texEnv = mkTex();
        fboA = mkFbo(texA); fboB = mkFbo(texB); rb = new Float32Array(W * H * 4); makeColorRT(); }
      function cleanup() { [texA, texB, texEnv].forEach(t => t && gl.deleteTexture(t));
        [fboA, fboB].forEach(f => f && gl.deleteFramebuffer(f)); if (colorRT) colorRT.dispose(); }

      function uploadState() { const b = new Float32Array(W * H * 4); cfg.packState(b, W, H);
        gl.bindTexture(gl.TEXTURE_2D, texA); gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, W, H, gl.RGBA, gl.FLOAT, b); api.dirty = false; }
      function uploadEnv() { if (!cfg.hasEnv) { api.envDirty = false; return; }
        const b = new Float32Array(W * H * 4); cfg.packEnv(b, W, H);
        gl.bindTexture(gl.TEXTURE_2D, texEnv); gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, W, H, gl.RGBA, gl.FLOAT, b); api.envDirty = false; }

      function bindState() {
        gl.bindVertexArray(vao);
        gl.viewport(0, 0, W, H);
        gl.disable(gl.BLEND); gl.disable(gl.DEPTH_TEST); gl.disable(gl.SCISSOR_TEST);
        gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, texA);
        gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, texEnv);
      }
      function finish() { gl.bindFramebuffer(gl.FRAMEBUFFER, null); gl.bindVertexArray(null); if (renderer.resetState) renderer.resetState(); }
      const resized = () => { const [c, r] = cfg.getSize(); return c !== W || r !== H; };
      const ensure = () => { if (resized()) { cleanup(); alloc(); api.dirty = api.envDirty = true; } };

      alloc();
      api.available = true;

      api.step = function () {                         // advance one generation (compute only)
        ensure();
        if (api.dirty) uploadState();
        if (api.envDirty) uploadEnv();
        gl.useProgram(prog); bindState();
        uSim.i1('uState', 0); uSim.i1('uEnv', 1);
        cfg.setSimUniforms(uSim, W, H);
        gl.bindFramebuffer(gl.FRAMEBUFFER, fboB);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        [texA, texB] = [texB, texA]; [fboA, fboB] = [fboB, fboA];   // result becomes next input
        finish();
      };

      api.refresh = function () {                      // colour current state into the display target
        ensure();
        if (api.dirty) uploadState();
        if (api.envDirty) uploadEnv();
        gl.useProgram(cprog); bindState();
        uColor.i1('uState', 0); uColor.i1('uEnv', 1);
        if (cfg.setColorUniforms) cfg.setColorUniforms(uColor, W, H);
        gl.bindFramebuffer(gl.FRAMEBUFFER, cfb);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        finish();
      };

      api.syncToCPU = function () {                    // read live state back into the CPU arrays
        gl.bindFramebuffer(gl.FRAMEBUFFER, fboA);      // texA holds current state (post-swap)
        gl.readPixels(0, 0, W, H, gl.RGBA, gl.FLOAT, rb);
        cfg.unpackState(rb, W, H);
        finish();
      };

      api.resize = function () { cleanup(); alloc(); api.dirty = api.envDirty = true; };
    } catch (e) {
      console.warn('LifeGPU init failed — falling back to CPU:', e);
      api.available = false; api.enabled = false;
      api.step = api.refresh = api.syncToCPU = api.resize = function () {};
    }
    return api;
  }

  root.LifeGPU = { create };
})(typeof self !== 'undefined' ? self : this);
