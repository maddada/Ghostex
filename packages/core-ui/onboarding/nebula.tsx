import { useEffect, useRef } from 'react';
import { compileShaderProgram, releaseWebGl } from './dark-veil';
import { prefersReducedMotion } from './stage';

const VERTEX_SHADER = 'attribute vec2 a;varying vec2 v;void main(){v=a*.5+.5;gl_Position=vec4(a,0.,1.);}';

const FRAGMENT_SHADER = `precision highp float;
varying vec2 v;uniform float t;uniform vec2 res;
float h(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}
float n(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);
  return mix(mix(h(i),h(i+vec2(1,0)),f.x),mix(h(i+vec2(0,1)),h(i+vec2(1,1)),f.x),f.y);}
float fbm(vec2 p){float s=0.,a=.5;for(int i=0;i<5;i++){s+=a*n(p);p=p*2.03+vec2(1.7,9.2);a*=.5;}return s;}
float band(vec2 p,vec2 c,float r,float w){float d=length(p-c)-r;return exp(-d*d/(w*w));}
void main(){
  float ys=1.-v.y;
  vec2 p=vec2(v.x*res.x/res.y,ys);
  float T=t*.03;
  vec2 w=vec2(fbm(p*1.6+vec2(T,0.)),fbm(p*1.6+vec2(4.3,-T)))-.5;
  vec2 q=p+w*.24;
  float s=0.;
  s+=band(q,vec2(1.30+.05*sin(T*1.3),-1.05),1.40,.040);
  s+=band(q,vec2(2.05,1.70+.05*cos(T)),1.12,.055)*.9;
  s+=band(q,vec2(0.35,1.55),0.95,.050)*.6;
  s+=band(q,vec2(1.62,0.30+.04*sin(T*1.7)),0.62,.028)*.55;
  s+=band(q,vec2(0.95,-0.35),0.80,.035)*.5;
  float cl=fbm(q*2.2+vec2(-T*.7,T*.4));
  s=min(s,1.1);
  float neb=s*(.35+.9*cl)+smoothstep(.5,.98,cl)*.24;
  float amt=.09;
  vec3 blue=vec3(.12,.23,.82);
  float l=dot(blue,vec3(.2126,.7152,.0722));
  blue=mix(blue,vec3(l*1.6),.62);
  vec3 col=blue*neb*amt*1.3;
  col+=vec3(.55,.68,1.)*pow(s,9.)*.10*amt;
  vec2 g=floor(v*res*.5);float r=h(g);
  col+=vec3(.75,.82,1.)*smoothstep(.9978,1.,r)*(.5+.5*sin(t*1.2+r*400.))*.18;
  vec3 field=(.044-.014*ys)*vec3(.88,.95,1.05);
  vec2 gg=vec2(v.x-.05,ys-.06);
  field+=exp(-(gg.x*gg.x/.10+gg.y*gg.y/.34))*.026*vec3(.5,.7,1.);
  gl_FragColor=vec4(col+field,1.);
}`;

function paintFallback(canvas: HTMLCanvasElement): void {
  const context = canvas.getContext('2d');
  if (!context) return;
  context.fillStyle = '#0b0c0f';
  context.fillRect(0, 0, canvas.width, canvas.height);
}

/** The faint blue nebula behind the whole stage (the prototype's `qm`), drawn at ~30 fps. */
export function Nebula({ active }: { active: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!active || !canvas) return;
    const gl = canvas.getContext('webgl', { antialias: false, preserveDrawingBuffer: true });
    if (!gl) {
      paintFallback(canvas);
      return;
    }
    const program = compileShaderProgram(gl, VERTEX_SHADER, FRAGMENT_SHADER);
    if (!program) {
      releaseWebGl(gl);
      paintFallback(canvas);
      return;
    }
    gl.useProgram(program);
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const position = gl.getAttribLocation(program, 'a');
    gl.enableVertexAttribArray(position);
    gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);
    const timeUniform = gl.getUniformLocation(program, 't');
    gl.uniform2f(gl.getUniformLocation(program, 'res'), canvas.width, canvas.height);
    gl.viewport(0, 0, canvas.width, canvas.height);
    const reduced = prefersReducedMotion();
    const startedAt = performance.now() - 40000;
    let lastDraw = -1e9;
    let frame = 0;
    const draw = (now: number) => {
      if (!reduced) frame = requestAnimationFrame(draw);
      if (now - lastDraw < 33) return;
      lastDraw = now;
      gl.uniform1f(timeUniform, reduced ? 40 : (now - startedAt) / 1000);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    };
    frame = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(frame);
      releaseWebGl(gl);
    };
  }, [active]);
  return <canvas ref={canvasRef} className='nebula' width={836} height={470} aria-hidden='true' />;
}
