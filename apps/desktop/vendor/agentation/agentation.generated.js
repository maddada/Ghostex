/*! Agentation 3.1.2 (fork maddada/agentation 2d958fb580e6a59c09ce738463eb349b971e9ba4), bundled for Ghostex with React, ReactDOM and modern-screenshot (MIT).
PolyForm Shield License 1.0.0

Copyright (c) 2026 Benji Taylor

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to use,
copy, modify, and distribute the Software, subject to the following conditions:

1. You may not use the Software to provide a product or service that competes
   with the Software or any product or service offered by the Licensor that
   includes the Software.

2. You may not remove or obscure any licensing, copyright, or other notices
   included in the Software.

3. If you distribute the Software or any derivative works, you must include a
   copy of this license.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

For more information, see https://polyformproject.org/licenses/shield/1.0.0
*/
"use strict";var __ghostexAgentation=(()=>{var k5=Object.create;var _a=Object.defineProperty;var S5=Object.getOwnPropertyDescriptor;var C5=Object.getOwnPropertyNames;var E5=Object.getPrototypeOf,M5=Object.prototype.hasOwnProperty;var Ao=(e,t)=>()=>(t||e((t={exports:{}}).exports,t),t.exports),T5=(e,t)=>{for(var n in t)_a(e,n,{get:t[n],enumerable:!0})},Hp=(e,t,n,o)=>{if(t&&typeof t=="object"||typeof t=="function")for(let r of C5(t))!M5.call(e,r)&&r!==n&&_a(e,r,{get:()=>t[r],enumerable:!(o=S5(t,r))||o.enumerable});return e};var ce=(e,t,n)=>(n=e!=null?k5(E5(e)):{},Hp(t||!e||!e.__esModule?_a(n,"default",{value:e,enumerable:!0}):n,e)),L5=e=>Hp(_a({},"__esModule",{value:!0}),e);var Zp=Ao(We=>{"use strict";var Rs=Symbol.for("react.element"),R5=Symbol.for("react.portal"),P5=Symbol.for("react.fragment"),N5=Symbol.for("react.strict_mode"),I5=Symbol.for("react.profiler"),D5=Symbol.for("react.provider"),$5=Symbol.for("react.context"),A5=Symbol.for("react.forward_ref"),F5=Symbol.for("react.suspense"),O5=Symbol.for("react.memo"),B5=Symbol.for("react.lazy"),Wp=Symbol.iterator;function z5(e){return e===null||typeof e!="object"?null:(e=Wp&&e[Wp]||e["@@iterator"],typeof e=="function"?e:null)}var Yp={isMounted:function(){return!1},enqueueForceUpdate:function(){},enqueueReplaceState:function(){},enqueueSetState:function(){}},Vp=Object.assign,Xp={};function xi(e,t,n){this.props=e,this.context=t,this.refs=Xp,this.updater=n||Yp}xi.prototype.isReactComponent={};xi.prototype.setState=function(e,t){if(typeof e!="object"&&typeof e!="function"&&e!=null)throw Error("setState(...): takes an object of state variables to update or a function which returns an object of state variables.");this.updater.enqueueSetState(this,e,t,"setState")};xi.prototype.forceUpdate=function(e){this.updater.enqueueForceUpdate(this,e,"forceUpdate")};function Qp(){}Qp.prototype=xi.prototype;function Hu(e,t,n){this.props=e,this.context=t,this.refs=Xp,this.updater=n||Yp}var Wu=Hu.prototype=new Qp;Wu.constructor=Hu;Vp(Wu,xi.prototype);Wu.isPureReactComponent=!0;var Up=Array.isArray,Gp=Object.prototype.hasOwnProperty,Uu={current:null},qp={key:!0,ref:!0,__self:!0,__source:!0};function Kp(e,t,n){var o,r={},i=null,s=null;if(t!=null)for(o in t.ref!==void 0&&(s=t.ref),t.key!==void 0&&(i=""+t.key),t)Gp.call(t,o)&&!qp.hasOwnProperty(o)&&(r[o]=t[o]);var l=arguments.length-2;if(l===1)r.children=n;else if(1<l){for(var a=Array(l),c=0;c<l;c++)a[c]=arguments[c+2];r.children=a}if(e&&e.defaultProps)for(o in l=e.defaultProps,l)r[o]===void 0&&(r[o]=l[o]);return{$$typeof:Rs,type:e,key:i,ref:s,props:r,_owner:Uu.current}}function H5(e,t){return{$$typeof:Rs,type:e.type,key:t,ref:e.ref,props:e.props,_owner:e._owner}}function ju(e){return typeof e=="object"&&e!==null&&e.$$typeof===Rs}function W5(e){var t={"=":"=0",":":"=2"};return"$"+e.replace(/[=:]/g,function(n){return t[n]})}var jp=/\/+/g;function zu(e,t){return typeof e=="object"&&e!==null&&e.key!=null?W5(""+e.key):t.toString(36)}function pa(e,t,n,o,r){var i=typeof e;(i==="undefined"||i==="boolean")&&(e=null);var s=!1;if(e===null)s=!0;else switch(i){case"string":case"number":s=!0;break;case"object":switch(e.$$typeof){case Rs:case R5:s=!0}}if(s)return s=e,r=r(s),e=o===""?"."+zu(s,0):o,Up(r)?(n="",e!=null&&(n=e.replace(jp,"$&/")+"/"),pa(r,t,n,"",function(c){return c})):r!=null&&(ju(r)&&(r=H5(r,n+(!r.key||s&&s.key===r.key?"":(""+r.key).replace(jp,"$&/")+"/")+e)),t.push(r)),1;if(s=0,o=o===""?".":o+":",Up(e))for(var l=0;l<e.length;l++){i=e[l];var a=o+zu(i,l);s+=pa(i,t,n,a,r)}else if(a=z5(e),typeof a=="function")for(e=a.call(e),l=0;!(i=e.next()).done;)i=i.value,a=o+zu(i,l++),s+=pa(i,t,n,a,r);else if(i==="object")throw t=String(e),Error("Objects are not valid as a React child (found: "+(t==="[object Object]"?"object with keys {"+Object.keys(e).join(", ")+"}":t)+"). If you meant to render a collection of children, use an array instead.");return s}function ma(e,t,n){if(e==null)return e;var o=[],r=0;return pa(e,o,"","",function(i){return t.call(n,i,r++)}),o}function U5(e){if(e._status===-1){var t=e._result;t=t(),t.then(function(n){(e._status===0||e._status===-1)&&(e._status=1,e._result=n)},function(n){(e._status===0||e._status===-1)&&(e._status=2,e._result=n)}),e._status===-1&&(e._status=0,e._result=t)}if(e._status===1)return e._result.default;throw e._result}var pn={current:null},fa={transition:null},j5={ReactCurrentDispatcher:pn,ReactCurrentBatchConfig:fa,ReactCurrentOwner:Uu};function Jp(){throw Error("act(...) is not supported in production builds of React.")}We.Children={map:ma,forEach:function(e,t,n){ma(e,function(){t.apply(this,arguments)},n)},count:function(e){var t=0;return ma(e,function(){t++}),t},toArray:function(e){return ma(e,function(t){return t})||[]},only:function(e){if(!ju(e))throw Error("React.Children.only expected to receive a single React element child.");return e}};We.Component=xi;We.Fragment=P5;We.Profiler=I5;We.PureComponent=Hu;We.StrictMode=N5;We.Suspense=F5;We.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED=j5;We.act=Jp;We.cloneElement=function(e,t,n){if(e==null)throw Error("React.cloneElement(...): The argument must be a React element, but you passed "+e+".");var o=Vp({},e.props),r=e.key,i=e.ref,s=e._owner;if(t!=null){if(t.ref!==void 0&&(i=t.ref,s=Uu.current),t.key!==void 0&&(r=""+t.key),e.type&&e.type.defaultProps)var l=e.type.defaultProps;for(a in t)Gp.call(t,a)&&!qp.hasOwnProperty(a)&&(o[a]=t[a]===void 0&&l!==void 0?l[a]:t[a])}var a=arguments.length-2;if(a===1)o.children=n;else if(1<a){l=Array(a);for(var c=0;c<a;c++)l[c]=arguments[c+2];o.children=l}return{$$typeof:Rs,type:e.type,key:r,ref:i,props:o,_owner:s}};We.createContext=function(e){return e={$$typeof:$5,_currentValue:e,_currentValue2:e,_threadCount:0,Provider:null,Consumer:null,_defaultValue:null,_globalName:null},e.Provider={$$typeof:D5,_context:e},e.Consumer=e};We.createElement=Kp;We.createFactory=function(e){var t=Kp.bind(null,e);return t.type=e,t};We.createRef=function(){return{current:null}};We.forwardRef=function(e){return{$$typeof:A5,render:e}};We.isValidElement=ju;We.lazy=function(e){return{$$typeof:B5,_payload:{_status:-1,_result:e},_init:U5}};We.memo=function(e,t){return{$$typeof:O5,type:e,compare:t===void 0?null:t}};We.startTransition=function(e){var t=fa.transition;fa.transition={};try{e()}finally{fa.transition=t}};We.unstable_act=Jp;We.useCallback=function(e,t){return pn.current.useCallback(e,t)};We.useContext=function(e){return pn.current.useContext(e)};We.useDebugValue=function(){};We.useDeferredValue=function(e){return pn.current.useDeferredValue(e)};We.useEffect=function(e,t){return pn.current.useEffect(e,t)};We.useId=function(){return pn.current.useId()};We.useImperativeHandle=function(e,t,n){return pn.current.useImperativeHandle(e,t,n)};We.useInsertionEffect=function(e,t){return pn.current.useInsertionEffect(e,t)};We.useLayoutEffect=function(e,t){return pn.current.useLayoutEffect(e,t)};We.useMemo=function(e,t){return pn.current.useMemo(e,t)};We.useReducer=function(e,t,n){return pn.current.useReducer(e,t,n)};We.useRef=function(e){return pn.current.useRef(e)};We.useState=function(e){return pn.current.useState(e)};We.useSyncExternalStore=function(e,t,n){return pn.current.useSyncExternalStore(e,t,n)};We.useTransition=function(){return pn.current.useTransition()};We.version="18.3.1"});var vt=Ao((Fk,ef)=>{"use strict";ef.exports=Zp()});var df=Ao(wt=>{"use strict";function Qu(e,t){var n=e.length;e.push(t);e:for(;0<n;){var o=n-1>>>1,r=e[o];if(0<ga(r,t))e[o]=t,e[n]=r,n=o;else break e}}function io(e){return e.length===0?null:e[0]}function ya(e){if(e.length===0)return null;var t=e[0],n=e.pop();if(n!==t){e[0]=n;e:for(var o=0,r=e.length,i=r>>>1;o<i;){var s=2*(o+1)-1,l=e[s],a=s+1,c=e[a];if(0>ga(l,n))a<r&&0>ga(c,l)?(e[o]=c,e[a]=n,o=a):(e[o]=l,e[s]=n,o=s);else if(a<r&&0>ga(c,n))e[o]=c,e[a]=n,o=a;else break e}}return t}function ga(e,t){var n=e.sortIndex-t.sortIndex;return n!==0?n:e.id-t.id}typeof performance=="object"&&typeof performance.now=="function"?(tf=performance,wt.unstable_now=function(){return tf.now()}):(Yu=Date,nf=Yu.now(),wt.unstable_now=function(){return Yu.now()-nf});var tf,Yu,nf,ko=[],Zo=[],Y5=1,Un=null,sn=3,xa=!1,Ur=!1,Ns=!1,sf=typeof setTimeout=="function"?setTimeout:null,lf=typeof clearTimeout=="function"?clearTimeout:null,of=typeof setImmediate<"u"?setImmediate:null;typeof navigator<"u"&&navigator.scheduling!==void 0&&navigator.scheduling.isInputPending!==void 0&&navigator.scheduling.isInputPending.bind(navigator.scheduling);function Gu(e){for(var t=io(Zo);t!==null;){if(t.callback===null)ya(Zo);else if(t.startTime<=e)ya(Zo),t.sortIndex=t.expirationTime,Qu(ko,t);else break;t=io(Zo)}}function qu(e){if(Ns=!1,Gu(e),!Ur)if(io(ko)!==null)Ur=!0,Ju(Ku);else{var t=io(Zo);t!==null&&Zu(qu,t.startTime-e)}}function Ku(e,t){Ur=!1,Ns&&(Ns=!1,lf(Is),Is=-1),xa=!0;var n=sn;try{for(Gu(t),Un=io(ko);Un!==null&&(!(Un.expirationTime>t)||e&&!uf());){var o=Un.callback;if(typeof o=="function"){Un.callback=null,sn=Un.priorityLevel;var r=o(Un.expirationTime<=t);t=wt.unstable_now(),typeof r=="function"?Un.callback=r:Un===io(ko)&&ya(ko),Gu(t)}else ya(ko);Un=io(ko)}if(Un!==null)var i=!0;else{var s=io(Zo);s!==null&&Zu(qu,s.startTime-t),i=!1}return i}finally{Un=null,sn=n,xa=!1}}var va=!1,ha=null,Is=-1,af=5,cf=-1;function uf(){return!(wt.unstable_now()-cf<af)}function Vu(){if(ha!==null){var e=wt.unstable_now();cf=e;var t=!0;try{t=ha(!0,e)}finally{t?Ps():(va=!1,ha=null)}}else va=!1}var Ps;typeof of=="function"?Ps=function(){of(Vu)}:typeof MessageChannel<"u"?(Xu=new MessageChannel,rf=Xu.port2,Xu.port1.onmessage=Vu,Ps=function(){rf.postMessage(null)}):Ps=function(){sf(Vu,0)};var Xu,rf;function Ju(e){ha=e,va||(va=!0,Ps())}function Zu(e,t){Is=sf(function(){e(wt.unstable_now())},t)}wt.unstable_IdlePriority=5;wt.unstable_ImmediatePriority=1;wt.unstable_LowPriority=4;wt.unstable_NormalPriority=3;wt.unstable_Profiling=null;wt.unstable_UserBlockingPriority=2;wt.unstable_cancelCallback=function(e){e.callback=null};wt.unstable_continueExecution=function(){Ur||xa||(Ur=!0,Ju(Ku))};wt.unstable_forceFrameRate=function(e){0>e||125<e?console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported"):af=0<e?Math.floor(1e3/e):5};wt.unstable_getCurrentPriorityLevel=function(){return sn};wt.unstable_getFirstCallbackNode=function(){return io(ko)};wt.unstable_next=function(e){switch(sn){case 1:case 2:case 3:var t=3;break;default:t=sn}var n=sn;sn=t;try{return e()}finally{sn=n}};wt.unstable_pauseExecution=function(){};wt.unstable_requestPaint=function(){};wt.unstable_runWithPriority=function(e,t){switch(e){case 1:case 2:case 3:case 4:case 5:break;default:e=3}var n=sn;sn=e;try{return t()}finally{sn=n}};wt.unstable_scheduleCallback=function(e,t,n){var o=wt.unstable_now();switch(typeof n=="object"&&n!==null?(n=n.delay,n=typeof n=="number"&&0<n?o+n:o):n=o,e){case 1:var r=-1;break;case 2:r=250;break;case 5:r=1073741823;break;case 4:r=1e4;break;default:r=5e3}return r=n+r,e={id:Y5++,callback:t,priorityLevel:e,startTime:n,expirationTime:r,sortIndex:-1},n>o?(e.sortIndex=n,Qu(Zo,e),io(ko)===null&&e===io(Zo)&&(Ns?(lf(Is),Is=-1):Ns=!0,Zu(qu,n-o))):(e.sortIndex=r,Qu(ko,e),Ur||xa||(Ur=!0,Ju(Ku))),e};wt.unstable_shouldYield=uf;wt.unstable_wrapCallback=function(e){var t=sn;return function(){var n=sn;sn=t;try{return e.apply(this,arguments)}finally{sn=n}}}});var mf=Ao((Bk,_f)=>{"use strict";_f.exports=df()});var h0=Ao(An=>{"use strict";var V5=vt(),Dn=mf();function Q(e){for(var t="https://reactjs.org/docs/error-decoder.html?invariant="+e,n=1;n<arguments.length;n++)t+="&args[]="+encodeURIComponent(arguments[n]);return"Minified React error #"+e+"; visit "+t+" for the full message or use the non-minified dev environment for full errors and additional helpful warnings."}var vg=new Set,tl={};function ni(e,t){zi(e,t),zi(e+"Capture",t)}function zi(e,t){for(tl[e]=t,e=0;e<t.length;e++)vg.add(t[e])}var Wo=!(typeof window>"u"||typeof window.document>"u"||typeof window.document.createElement>"u"),bd=Object.prototype.hasOwnProperty,X5=/^[:A-Z_a-z\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u02FF\u0370-\u037D\u037F-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD][:A-Z_a-z\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u02FF\u0370-\u037D\u037F-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD\-.0-9\u00B7\u0300-\u036F\u203F-\u2040]*$/,pf={},ff={};function Q5(e){return bd.call(ff,e)?!0:bd.call(pf,e)?!1:X5.test(e)?ff[e]=!0:(pf[e]=!0,!1)}function G5(e,t,n,o){if(n!==null&&n.type===0)return!1;switch(typeof t){case"function":case"symbol":return!0;case"boolean":return o?!1:n!==null?!n.acceptsBooleans:(e=e.toLowerCase().slice(0,5),e!=="data-"&&e!=="aria-");default:return!1}}function q5(e,t,n,o){if(t===null||typeof t>"u"||G5(e,t,n,o))return!0;if(o)return!1;if(n!==null)switch(n.type){case 3:return!t;case 4:return t===!1;case 5:return isNaN(t);case 6:return isNaN(t)||1>t}return!1}function hn(e,t,n,o,r,i,s){this.acceptsBooleans=t===2||t===3||t===4,this.attributeName=o,this.attributeNamespace=r,this.mustUseProperty=n,this.propertyName=e,this.type=t,this.sanitizeURL=i,this.removeEmptyString=s}var tn={};"children dangerouslySetInnerHTML defaultValue defaultChecked innerHTML suppressContentEditableWarning suppressHydrationWarning style".split(" ").forEach(function(e){tn[e]=new hn(e,0,!1,e,null,!1,!1)});[["acceptCharset","accept-charset"],["className","class"],["htmlFor","for"],["httpEquiv","http-equiv"]].forEach(function(e){var t=e[0];tn[t]=new hn(t,1,!1,e[1],null,!1,!1)});["contentEditable","draggable","spellCheck","value"].forEach(function(e){tn[e]=new hn(e,2,!1,e.toLowerCase(),null,!1,!1)});["autoReverse","externalResourcesRequired","focusable","preserveAlpha"].forEach(function(e){tn[e]=new hn(e,2,!1,e,null,!1,!1)});"allowFullScreen async autoFocus autoPlay controls default defer disabled disablePictureInPicture disableRemotePlayback formNoValidate hidden loop noModule noValidate open playsInline readOnly required reversed scoped seamless itemScope".split(" ").forEach(function(e){tn[e]=new hn(e,3,!1,e.toLowerCase(),null,!1,!1)});["checked","multiple","muted","selected"].forEach(function(e){tn[e]=new hn(e,3,!0,e,null,!1,!1)});["capture","download"].forEach(function(e){tn[e]=new hn(e,4,!1,e,null,!1,!1)});["cols","rows","size","span"].forEach(function(e){tn[e]=new hn(e,6,!1,e,null,!1,!1)});["rowSpan","start"].forEach(function(e){tn[e]=new hn(e,5,!1,e.toLowerCase(),null,!1,!1)});var p_=/[\-:]([a-z])/g;function f_(e){return e[1].toUpperCase()}"accent-height alignment-baseline arabic-form baseline-shift cap-height clip-path clip-rule color-interpolation color-interpolation-filters color-profile color-rendering dominant-baseline enable-background fill-opacity fill-rule flood-color flood-opacity font-family font-size font-size-adjust font-stretch font-style font-variant font-weight glyph-name glyph-orientation-horizontal glyph-orientation-vertical horiz-adv-x horiz-origin-x image-rendering letter-spacing lighting-color marker-end marker-mid marker-start overline-position overline-thickness paint-order panose-1 pointer-events rendering-intent shape-rendering stop-color stop-opacity strikethrough-position strikethrough-thickness stroke-dasharray stroke-dashoffset stroke-linecap stroke-linejoin stroke-miterlimit stroke-opacity stroke-width text-anchor text-decoration text-rendering underline-position underline-thickness unicode-bidi unicode-range units-per-em v-alphabetic v-hanging v-ideographic v-mathematical vector-effect vert-adv-y vert-origin-x vert-origin-y word-spacing writing-mode xmlns:xlink x-height".split(" ").forEach(function(e){var t=e.replace(p_,f_);tn[t]=new hn(t,1,!1,e,null,!1,!1)});"xlink:actuate xlink:arcrole xlink:role xlink:show xlink:title xlink:type".split(" ").forEach(function(e){var t=e.replace(p_,f_);tn[t]=new hn(t,1,!1,e,"http://www.w3.org/1999/xlink",!1,!1)});["xml:base","xml:lang","xml:space"].forEach(function(e){var t=e.replace(p_,f_);tn[t]=new hn(t,1,!1,e,"http://www.w3.org/XML/1998/namespace",!1,!1)});["tabIndex","crossOrigin"].forEach(function(e){tn[e]=new hn(e,1,!1,e.toLowerCase(),null,!1,!1)});tn.xlinkHref=new hn("xlinkHref",1,!1,"xlink:href","http://www.w3.org/1999/xlink",!0,!1);["src","href","action","formAction"].forEach(function(e){tn[e]=new hn(e,1,!1,e.toLowerCase(),null,!0,!0)});function g_(e,t,n,o){var r=tn.hasOwnProperty(t)?tn[t]:null;(r!==null?r.type!==0:o||!(2<t.length)||t[0]!=="o"&&t[0]!=="O"||t[1]!=="n"&&t[1]!=="N")&&(q5(t,n,r,o)&&(n=null),o||r===null?Q5(t)&&(n===null?e.removeAttribute(t):e.setAttribute(t,""+n)):r.mustUseProperty?e[r.propertyName]=n===null?r.type===3?!1:"":n:(t=r.attributeName,o=r.attributeNamespace,n===null?e.removeAttribute(t):(r=r.type,n=r===3||r===4&&n===!0?"":""+n,o?e.setAttributeNS(o,t,n):e.setAttribute(t,n))))}var Vo=V5.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED,wa=Symbol.for("react.element"),bi=Symbol.for("react.portal"),ki=Symbol.for("react.fragment"),h_=Symbol.for("react.strict_mode"),kd=Symbol.for("react.profiler"),wg=Symbol.for("react.provider"),bg=Symbol.for("react.context"),y_=Symbol.for("react.forward_ref"),Sd=Symbol.for("react.suspense"),Cd=Symbol.for("react.suspense_list"),x_=Symbol.for("react.memo"),tr=Symbol.for("react.lazy"),kg=Symbol.for("react.offscreen"),gf=Symbol.iterator;function Ds(e){return e===null||typeof e!="object"?null:(e=gf&&e[gf]||e["@@iterator"],typeof e=="function"?e:null)}var Pt=Object.assign,ed;function Ws(e){if(ed===void 0)try{throw Error()}catch(n){var t=n.stack.trim().match(/\n( *(at )?)/);ed=t&&t[1]||""}return`
`+ed+e}var td=!1;function nd(e,t){if(!e||td)return"";td=!0;var n=Error.prepareStackTrace;Error.prepareStackTrace=void 0;try{if(t)if(t=function(){throw Error()},Object.defineProperty(t.prototype,"props",{set:function(){throw Error()}}),typeof Reflect=="object"&&Reflect.construct){try{Reflect.construct(t,[])}catch(c){var o=c}Reflect.construct(e,[],t)}else{try{t.call()}catch(c){o=c}e.call(t.prototype)}else{try{throw Error()}catch(c){o=c}e()}}catch(c){if(c&&o&&typeof c.stack=="string"){for(var r=c.stack.split(`
`),i=o.stack.split(`
`),s=r.length-1,l=i.length-1;1<=s&&0<=l&&r[s]!==i[l];)l--;for(;1<=s&&0<=l;s--,l--)if(r[s]!==i[l]){if(s!==1||l!==1)do if(s--,l--,0>l||r[s]!==i[l]){var a=`
`+r[s].replace(" at new "," at ");return e.displayName&&a.includes("<anonymous>")&&(a=a.replace("<anonymous>",e.displayName)),a}while(1<=s&&0<=l);break}}}finally{td=!1,Error.prepareStackTrace=n}return(e=e?e.displayName||e.name:"")?Ws(e):""}function K5(e){switch(e.tag){case 5:return Ws(e.type);case 16:return Ws("Lazy");case 13:return Ws("Suspense");case 19:return Ws("SuspenseList");case 0:case 2:case 15:return e=nd(e.type,!1),e;case 11:return e=nd(e.type.render,!1),e;case 1:return e=nd(e.type,!0),e;default:return""}}function Ed(e){if(e==null)return null;if(typeof e=="function")return e.displayName||e.name||null;if(typeof e=="string")return e;switch(e){case ki:return"Fragment";case bi:return"Portal";case kd:return"Profiler";case h_:return"StrictMode";case Sd:return"Suspense";case Cd:return"SuspenseList"}if(typeof e=="object")switch(e.$$typeof){case bg:return(e.displayName||"Context")+".Consumer";case wg:return(e._context.displayName||"Context")+".Provider";case y_:var t=e.render;return e=e.displayName,e||(e=t.displayName||t.name||"",e=e!==""?"ForwardRef("+e+")":"ForwardRef"),e;case x_:return t=e.displayName||null,t!==null?t:Ed(e.type)||"Memo";case tr:t=e._payload,e=e._init;try{return Ed(e(t))}catch{}}return null}function J5(e){var t=e.type;switch(e.tag){case 24:return"Cache";case 9:return(t.displayName||"Context")+".Consumer";case 10:return(t._context.displayName||"Context")+".Provider";case 18:return"DehydratedFragment";case 11:return e=t.render,e=e.displayName||e.name||"",t.displayName||(e!==""?"ForwardRef("+e+")":"ForwardRef");case 7:return"Fragment";case 5:return t;case 4:return"Portal";case 3:return"Root";case 6:return"Text";case 16:return Ed(t);case 8:return t===h_?"StrictMode":"Mode";case 22:return"Offscreen";case 12:return"Profiler";case 21:return"Scope";case 13:return"Suspense";case 19:return"SuspenseList";case 25:return"TracingMarker";case 1:case 0:case 17:case 2:case 14:case 15:if(typeof t=="function")return t.displayName||t.name||null;if(typeof t=="string")return t}return null}function fr(e){switch(typeof e){case"boolean":case"number":case"string":case"undefined":return e;case"object":return e;default:return""}}function Sg(e){var t=e.type;return(e=e.nodeName)&&e.toLowerCase()==="input"&&(t==="checkbox"||t==="radio")}function Z5(e){var t=Sg(e)?"checked":"value",n=Object.getOwnPropertyDescriptor(e.constructor.prototype,t),o=""+e[t];if(!e.hasOwnProperty(t)&&typeof n<"u"&&typeof n.get=="function"&&typeof n.set=="function"){var r=n.get,i=n.set;return Object.defineProperty(e,t,{configurable:!0,get:function(){return r.call(this)},set:function(s){o=""+s,i.call(this,s)}}),Object.defineProperty(e,t,{enumerable:n.enumerable}),{getValue:function(){return o},setValue:function(s){o=""+s},stopTracking:function(){e._valueTracker=null,delete e[t]}}}}function ba(e){e._valueTracker||(e._valueTracker=Z5(e))}function Cg(e){if(!e)return!1;var t=e._valueTracker;if(!t)return!0;var n=t.getValue(),o="";return e&&(o=Sg(e)?e.checked?"true":"false":e.value),e=o,e!==n?(t.setValue(e),!0):!1}function qa(e){if(e=e||(typeof document<"u"?document:void 0),typeof e>"u")return null;try{return e.activeElement||e.body}catch{return e.body}}function Md(e,t){var n=t.checked;return Pt({},t,{defaultChecked:void 0,defaultValue:void 0,value:void 0,checked:n??e._wrapperState.initialChecked})}function hf(e,t){var n=t.defaultValue==null?"":t.defaultValue,o=t.checked!=null?t.checked:t.defaultChecked;n=fr(t.value!=null?t.value:n),e._wrapperState={initialChecked:o,initialValue:n,controlled:t.type==="checkbox"||t.type==="radio"?t.checked!=null:t.value!=null}}function Eg(e,t){t=t.checked,t!=null&&g_(e,"checked",t,!1)}function Td(e,t){Eg(e,t);var n=fr(t.value),o=t.type;if(n!=null)o==="number"?(n===0&&e.value===""||e.value!=n)&&(e.value=""+n):e.value!==""+n&&(e.value=""+n);else if(o==="submit"||o==="reset"){e.removeAttribute("value");return}t.hasOwnProperty("value")?Ld(e,t.type,n):t.hasOwnProperty("defaultValue")&&Ld(e,t.type,fr(t.defaultValue)),t.checked==null&&t.defaultChecked!=null&&(e.defaultChecked=!!t.defaultChecked)}function yf(e,t,n){if(t.hasOwnProperty("value")||t.hasOwnProperty("defaultValue")){var o=t.type;if(!(o!=="submit"&&o!=="reset"||t.value!==void 0&&t.value!==null))return;t=""+e._wrapperState.initialValue,n||t===e.value||(e.value=t),e.defaultValue=t}n=e.name,n!==""&&(e.name=""),e.defaultChecked=!!e._wrapperState.initialChecked,n!==""&&(e.name=n)}function Ld(e,t,n){(t!=="number"||qa(e.ownerDocument)!==e)&&(n==null?e.defaultValue=""+e._wrapperState.initialValue:e.defaultValue!==""+n&&(e.defaultValue=""+n))}var Us=Array.isArray;function Di(e,t,n,o){if(e=e.options,t){t={};for(var r=0;r<n.length;r++)t["$"+n[r]]=!0;for(n=0;n<e.length;n++)r=t.hasOwnProperty("$"+e[n].value),e[n].selected!==r&&(e[n].selected=r),r&&o&&(e[n].defaultSelected=!0)}else{for(n=""+fr(n),t=null,r=0;r<e.length;r++){if(e[r].value===n){e[r].selected=!0,o&&(e[r].defaultSelected=!0);return}t!==null||e[r].disabled||(t=e[r])}t!==null&&(t.selected=!0)}}function Rd(e,t){if(t.dangerouslySetInnerHTML!=null)throw Error(Q(91));return Pt({},t,{value:void 0,defaultValue:void 0,children:""+e._wrapperState.initialValue})}function xf(e,t){var n=t.value;if(n==null){if(n=t.children,t=t.defaultValue,n!=null){if(t!=null)throw Error(Q(92));if(Us(n)){if(1<n.length)throw Error(Q(93));n=n[0]}t=n}t==null&&(t=""),n=t}e._wrapperState={initialValue:fr(n)}}function Mg(e,t){var n=fr(t.value),o=fr(t.defaultValue);n!=null&&(n=""+n,n!==e.value&&(e.value=n),t.defaultValue==null&&e.defaultValue!==n&&(e.defaultValue=n)),o!=null&&(e.defaultValue=""+o)}function vf(e){var t=e.textContent;t===e._wrapperState.initialValue&&t!==""&&t!==null&&(e.value=t)}function Tg(e){switch(e){case"svg":return"http://www.w3.org/2000/svg";case"math":return"http://www.w3.org/1998/Math/MathML";default:return"http://www.w3.org/1999/xhtml"}}function Pd(e,t){return e==null||e==="http://www.w3.org/1999/xhtml"?Tg(t):e==="http://www.w3.org/2000/svg"&&t==="foreignObject"?"http://www.w3.org/1999/xhtml":e}var ka,Lg=(function(e){return typeof MSApp<"u"&&MSApp.execUnsafeLocalFunction?function(t,n,o,r){MSApp.execUnsafeLocalFunction(function(){return e(t,n,o,r)})}:e})(function(e,t){if(e.namespaceURI!=="http://www.w3.org/2000/svg"||"innerHTML"in e)e.innerHTML=t;else{for(ka=ka||document.createElement("div"),ka.innerHTML="<svg>"+t.valueOf().toString()+"</svg>",t=ka.firstChild;e.firstChild;)e.removeChild(e.firstChild);for(;t.firstChild;)e.appendChild(t.firstChild)}});function nl(e,t){if(t){var n=e.firstChild;if(n&&n===e.lastChild&&n.nodeType===3){n.nodeValue=t;return}}e.textContent=t}var Vs={animationIterationCount:!0,aspectRatio:!0,borderImageOutset:!0,borderImageSlice:!0,borderImageWidth:!0,boxFlex:!0,boxFlexGroup:!0,boxOrdinalGroup:!0,columnCount:!0,columns:!0,flex:!0,flexGrow:!0,flexPositive:!0,flexShrink:!0,flexNegative:!0,flexOrder:!0,gridArea:!0,gridRow:!0,gridRowEnd:!0,gridRowSpan:!0,gridRowStart:!0,gridColumn:!0,gridColumnEnd:!0,gridColumnSpan:!0,gridColumnStart:!0,fontWeight:!0,lineClamp:!0,lineHeight:!0,opacity:!0,order:!0,orphans:!0,tabSize:!0,widows:!0,zIndex:!0,zoom:!0,fillOpacity:!0,floodOpacity:!0,stopOpacity:!0,strokeDasharray:!0,strokeDashoffset:!0,strokeMiterlimit:!0,strokeOpacity:!0,strokeWidth:!0},e2=["Webkit","ms","Moz","O"];Object.keys(Vs).forEach(function(e){e2.forEach(function(t){t=t+e.charAt(0).toUpperCase()+e.substring(1),Vs[t]=Vs[e]})});function Rg(e,t,n){return t==null||typeof t=="boolean"||t===""?"":n||typeof t!="number"||t===0||Vs.hasOwnProperty(e)&&Vs[e]?(""+t).trim():t+"px"}function Pg(e,t){e=e.style;for(var n in t)if(t.hasOwnProperty(n)){var o=n.indexOf("--")===0,r=Rg(n,t[n],o);n==="float"&&(n="cssFloat"),o?e.setProperty(n,r):e[n]=r}}var t2=Pt({menuitem:!0},{area:!0,base:!0,br:!0,col:!0,embed:!0,hr:!0,img:!0,input:!0,keygen:!0,link:!0,meta:!0,param:!0,source:!0,track:!0,wbr:!0});function Nd(e,t){if(t){if(t2[e]&&(t.children!=null||t.dangerouslySetInnerHTML!=null))throw Error(Q(137,e));if(t.dangerouslySetInnerHTML!=null){if(t.children!=null)throw Error(Q(60));if(typeof t.dangerouslySetInnerHTML!="object"||!("__html"in t.dangerouslySetInnerHTML))throw Error(Q(61))}if(t.style!=null&&typeof t.style!="object")throw Error(Q(62))}}function Id(e,t){if(e.indexOf("-")===-1)return typeof t.is=="string";switch(e){case"annotation-xml":case"color-profile":case"font-face":case"font-face-src":case"font-face-uri":case"font-face-format":case"font-face-name":case"missing-glyph":return!1;default:return!0}}var Dd=null;function v_(e){return e=e.target||e.srcElement||window,e.correspondingUseElement&&(e=e.correspondingUseElement),e.nodeType===3?e.parentNode:e}var $d=null,$i=null,Ai=null;function wf(e){if(e=vl(e)){if(typeof $d!="function")throw Error(Q(280));var t=e.stateNode;t&&(t=Sc(t),$d(e.stateNode,e.type,t))}}function Ng(e){$i?Ai?Ai.push(e):Ai=[e]:$i=e}function Ig(){if($i){var e=$i,t=Ai;if(Ai=$i=null,wf(e),t)for(e=0;e<t.length;e++)wf(t[e])}}function Dg(e,t){return e(t)}function $g(){}var od=!1;function Ag(e,t,n){if(od)return e(t,n);od=!0;try{return Dg(e,t,n)}finally{od=!1,($i!==null||Ai!==null)&&($g(),Ig())}}function ol(e,t){var n=e.stateNode;if(n===null)return null;var o=Sc(n);if(o===null)return null;n=o[t];e:switch(t){case"onClick":case"onClickCapture":case"onDoubleClick":case"onDoubleClickCapture":case"onMouseDown":case"onMouseDownCapture":case"onMouseMove":case"onMouseMoveCapture":case"onMouseUp":case"onMouseUpCapture":case"onMouseEnter":(o=!o.disabled)||(e=e.type,o=!(e==="button"||e==="input"||e==="select"||e==="textarea")),e=!o;break e;default:e=!1}if(e)return null;if(n&&typeof n!="function")throw Error(Q(231,t,typeof n));return n}var Ad=!1;if(Wo)try{vi={},Object.defineProperty(vi,"passive",{get:function(){Ad=!0}}),window.addEventListener("test",vi,vi),window.removeEventListener("test",vi,vi)}catch{Ad=!1}var vi;function n2(e,t,n,o,r,i,s,l,a){var c=Array.prototype.slice.call(arguments,3);try{t.apply(n,c)}catch(d){this.onError(d)}}var Xs=!1,Ka=null,Ja=!1,Fd=null,o2={onError:function(e){Xs=!0,Ka=e}};function r2(e,t,n,o,r,i,s,l,a){Xs=!1,Ka=null,n2.apply(o2,arguments)}function i2(e,t,n,o,r,i,s,l,a){if(r2.apply(this,arguments),Xs){if(Xs){var c=Ka;Xs=!1,Ka=null}else throw Error(Q(198));Ja||(Ja=!0,Fd=c)}}function oi(e){var t=e,n=e;if(e.alternate)for(;t.return;)t=t.return;else{e=t;do t=e,(t.flags&4098)!==0&&(n=t.return),e=t.return;while(e)}return t.tag===3?n:null}function Fg(e){if(e.tag===13){var t=e.memoizedState;if(t===null&&(e=e.alternate,e!==null&&(t=e.memoizedState)),t!==null)return t.dehydrated}return null}function bf(e){if(oi(e)!==e)throw Error(Q(188))}function s2(e){var t=e.alternate;if(!t){if(t=oi(e),t===null)throw Error(Q(188));return t!==e?null:e}for(var n=e,o=t;;){var r=n.return;if(r===null)break;var i=r.alternate;if(i===null){if(o=r.return,o!==null){n=o;continue}break}if(r.child===i.child){for(i=r.child;i;){if(i===n)return bf(r),e;if(i===o)return bf(r),t;i=i.sibling}throw Error(Q(188))}if(n.return!==o.return)n=r,o=i;else{for(var s=!1,l=r.child;l;){if(l===n){s=!0,n=r,o=i;break}if(l===o){s=!0,o=r,n=i;break}l=l.sibling}if(!s){for(l=i.child;l;){if(l===n){s=!0,n=i,o=r;break}if(l===o){s=!0,o=i,n=r;break}l=l.sibling}if(!s)throw Error(Q(189))}}if(n.alternate!==o)throw Error(Q(190))}if(n.tag!==3)throw Error(Q(188));return n.stateNode.current===n?e:t}function Og(e){return e=s2(e),e!==null?Bg(e):null}function Bg(e){if(e.tag===5||e.tag===6)return e;for(e=e.child;e!==null;){var t=Bg(e);if(t!==null)return t;e=e.sibling}return null}var zg=Dn.unstable_scheduleCallback,kf=Dn.unstable_cancelCallback,l2=Dn.unstable_shouldYield,a2=Dn.unstable_requestPaint,At=Dn.unstable_now,c2=Dn.unstable_getCurrentPriorityLevel,w_=Dn.unstable_ImmediatePriority,Hg=Dn.unstable_UserBlockingPriority,Za=Dn.unstable_NormalPriority,u2=Dn.unstable_LowPriority,Wg=Dn.unstable_IdlePriority,vc=null,Mo=null;function d2(e){if(Mo&&typeof Mo.onCommitFiberRoot=="function")try{Mo.onCommitFiberRoot(vc,e,void 0,(e.current.flags&128)===128)}catch{}}var uo=Math.clz32?Math.clz32:p2,_2=Math.log,m2=Math.LN2;function p2(e){return e>>>=0,e===0?32:31-(_2(e)/m2|0)|0}var Sa=64,Ca=4194304;function js(e){switch(e&-e){case 1:return 1;case 2:return 2;case 4:return 4;case 8:return 8;case 16:return 16;case 32:return 32;case 64:case 128:case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:case 262144:case 524288:case 1048576:case 2097152:return e&4194240;case 4194304:case 8388608:case 16777216:case 33554432:case 67108864:return e&130023424;case 134217728:return 134217728;case 268435456:return 268435456;case 536870912:return 536870912;case 1073741824:return 1073741824;default:return e}}function ec(e,t){var n=e.pendingLanes;if(n===0)return 0;var o=0,r=e.suspendedLanes,i=e.pingedLanes,s=n&268435455;if(s!==0){var l=s&~r;l!==0?o=js(l):(i&=s,i!==0&&(o=js(i)))}else s=n&~r,s!==0?o=js(s):i!==0&&(o=js(i));if(o===0)return 0;if(t!==0&&t!==o&&(t&r)===0&&(r=o&-o,i=t&-t,r>=i||r===16&&(i&4194240)!==0))return t;if((o&4)!==0&&(o|=n&16),t=e.entangledLanes,t!==0)for(e=e.entanglements,t&=o;0<t;)n=31-uo(t),r=1<<n,o|=e[n],t&=~r;return o}function f2(e,t){switch(e){case 1:case 2:case 4:return t+250;case 8:case 16:case 32:case 64:case 128:case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:case 262144:case 524288:case 1048576:case 2097152:return t+5e3;case 4194304:case 8388608:case 16777216:case 33554432:case 67108864:return-1;case 134217728:case 268435456:case 536870912:case 1073741824:return-1;default:return-1}}function g2(e,t){for(var n=e.suspendedLanes,o=e.pingedLanes,r=e.expirationTimes,i=e.pendingLanes;0<i;){var s=31-uo(i),l=1<<s,a=r[s];a===-1?((l&n)===0||(l&o)!==0)&&(r[s]=f2(l,t)):a<=t&&(e.expiredLanes|=l),i&=~l}}function Od(e){return e=e.pendingLanes&-1073741825,e!==0?e:e&1073741824?1073741824:0}function Ug(){var e=Sa;return Sa<<=1,(Sa&4194240)===0&&(Sa=64),e}function rd(e){for(var t=[],n=0;31>n;n++)t.push(e);return t}function yl(e,t,n){e.pendingLanes|=t,t!==536870912&&(e.suspendedLanes=0,e.pingedLanes=0),e=e.eventTimes,t=31-uo(t),e[t]=n}function h2(e,t){var n=e.pendingLanes&~t;e.pendingLanes=t,e.suspendedLanes=0,e.pingedLanes=0,e.expiredLanes&=t,e.mutableReadLanes&=t,e.entangledLanes&=t,t=e.entanglements;var o=e.eventTimes;for(e=e.expirationTimes;0<n;){var r=31-uo(n),i=1<<r;t[r]=0,o[r]=-1,e[r]=-1,n&=~i}}function b_(e,t){var n=e.entangledLanes|=t;for(e=e.entanglements;n;){var o=31-uo(n),r=1<<o;r&t|e[o]&t&&(e[o]|=t),n&=~r}}var mt=0;function jg(e){return e&=-e,1<e?4<e?(e&268435455)!==0?16:536870912:4:1}var Yg,k_,Vg,Xg,Qg,Bd=!1,Ea=[],lr=null,ar=null,cr=null,rl=new Map,il=new Map,or=[],y2="mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset submit".split(" ");function Sf(e,t){switch(e){case"focusin":case"focusout":lr=null;break;case"dragenter":case"dragleave":ar=null;break;case"mouseover":case"mouseout":cr=null;break;case"pointerover":case"pointerout":rl.delete(t.pointerId);break;case"gotpointercapture":case"lostpointercapture":il.delete(t.pointerId)}}function $s(e,t,n,o,r,i){return e===null||e.nativeEvent!==i?(e={blockedOn:t,domEventName:n,eventSystemFlags:o,nativeEvent:i,targetContainers:[r]},t!==null&&(t=vl(t),t!==null&&k_(t)),e):(e.eventSystemFlags|=o,t=e.targetContainers,r!==null&&t.indexOf(r)===-1&&t.push(r),e)}function x2(e,t,n,o,r){switch(t){case"focusin":return lr=$s(lr,e,t,n,o,r),!0;case"dragenter":return ar=$s(ar,e,t,n,o,r),!0;case"mouseover":return cr=$s(cr,e,t,n,o,r),!0;case"pointerover":var i=r.pointerId;return rl.set(i,$s(rl.get(i)||null,e,t,n,o,r)),!0;case"gotpointercapture":return i=r.pointerId,il.set(i,$s(il.get(i)||null,e,t,n,o,r)),!0}return!1}function Gg(e){var t=Vr(e.target);if(t!==null){var n=oi(t);if(n!==null){if(t=n.tag,t===13){if(t=Fg(n),t!==null){e.blockedOn=t,Qg(e.priority,function(){Vg(n)});return}}else if(t===3&&n.stateNode.current.memoizedState.isDehydrated){e.blockedOn=n.tag===3?n.stateNode.containerInfo:null;return}}}e.blockedOn=null}function za(e){if(e.blockedOn!==null)return!1;for(var t=e.targetContainers;0<t.length;){var n=zd(e.domEventName,e.eventSystemFlags,t[0],e.nativeEvent);if(n===null){n=e.nativeEvent;var o=new n.constructor(n.type,n);Dd=o,n.target.dispatchEvent(o),Dd=null}else return t=vl(n),t!==null&&k_(t),e.blockedOn=n,!1;t.shift()}return!0}function Cf(e,t,n){za(e)&&n.delete(t)}function v2(){Bd=!1,lr!==null&&za(lr)&&(lr=null),ar!==null&&za(ar)&&(ar=null),cr!==null&&za(cr)&&(cr=null),rl.forEach(Cf),il.forEach(Cf)}function As(e,t){e.blockedOn===t&&(e.blockedOn=null,Bd||(Bd=!0,Dn.unstable_scheduleCallback(Dn.unstable_NormalPriority,v2)))}function sl(e){function t(r){return As(r,e)}if(0<Ea.length){As(Ea[0],e);for(var n=1;n<Ea.length;n++){var o=Ea[n];o.blockedOn===e&&(o.blockedOn=null)}}for(lr!==null&&As(lr,e),ar!==null&&As(ar,e),cr!==null&&As(cr,e),rl.forEach(t),il.forEach(t),n=0;n<or.length;n++)o=or[n],o.blockedOn===e&&(o.blockedOn=null);for(;0<or.length&&(n=or[0],n.blockedOn===null);)Gg(n),n.blockedOn===null&&or.shift()}var Fi=Vo.ReactCurrentBatchConfig,tc=!0;function w2(e,t,n,o){var r=mt,i=Fi.transition;Fi.transition=null;try{mt=1,S_(e,t,n,o)}finally{mt=r,Fi.transition=i}}function b2(e,t,n,o){var r=mt,i=Fi.transition;Fi.transition=null;try{mt=4,S_(e,t,n,o)}finally{mt=r,Fi.transition=i}}function S_(e,t,n,o){if(tc){var r=zd(e,t,n,o);if(r===null)dd(e,t,o,nc,n),Sf(e,o);else if(x2(r,e,t,n,o))o.stopPropagation();else if(Sf(e,o),t&4&&-1<y2.indexOf(e)){for(;r!==null;){var i=vl(r);if(i!==null&&Yg(i),i=zd(e,t,n,o),i===null&&dd(e,t,o,nc,n),i===r)break;r=i}r!==null&&o.stopPropagation()}else dd(e,t,o,null,n)}}var nc=null;function zd(e,t,n,o){if(nc=null,e=v_(o),e=Vr(e),e!==null)if(t=oi(e),t===null)e=null;else if(n=t.tag,n===13){if(e=Fg(t),e!==null)return e;e=null}else if(n===3){if(t.stateNode.current.memoizedState.isDehydrated)return t.tag===3?t.stateNode.containerInfo:null;e=null}else t!==e&&(e=null);return nc=e,null}function qg(e){switch(e){case"cancel":case"click":case"close":case"contextmenu":case"copy":case"cut":case"auxclick":case"dblclick":case"dragend":case"dragstart":case"drop":case"focusin":case"focusout":case"input":case"invalid":case"keydown":case"keypress":case"keyup":case"mousedown":case"mouseup":case"paste":case"pause":case"play":case"pointercancel":case"pointerdown":case"pointerup":case"ratechange":case"reset":case"resize":case"seeked":case"submit":case"touchcancel":case"touchend":case"touchstart":case"volumechange":case"change":case"selectionchange":case"textInput":case"compositionstart":case"compositionend":case"compositionupdate":case"beforeblur":case"afterblur":case"beforeinput":case"blur":case"fullscreenchange":case"focus":case"hashchange":case"popstate":case"select":case"selectstart":return 1;case"drag":case"dragenter":case"dragexit":case"dragleave":case"dragover":case"mousemove":case"mouseout":case"mouseover":case"pointermove":case"pointerout":case"pointerover":case"scroll":case"toggle":case"touchmove":case"wheel":case"mouseenter":case"mouseleave":case"pointerenter":case"pointerleave":return 4;case"message":switch(c2()){case w_:return 1;case Hg:return 4;case Za:case u2:return 16;case Wg:return 536870912;default:return 16}default:return 16}}var ir=null,C_=null,Ha=null;function Kg(){if(Ha)return Ha;var e,t=C_,n=t.length,o,r="value"in ir?ir.value:ir.textContent,i=r.length;for(e=0;e<n&&t[e]===r[e];e++);var s=n-e;for(o=1;o<=s&&t[n-o]===r[i-o];o++);return Ha=r.slice(e,1<o?1-o:void 0)}function Wa(e){var t=e.keyCode;return"charCode"in e?(e=e.charCode,e===0&&t===13&&(e=13)):e=t,e===10&&(e=13),32<=e||e===13?e:0}function Ma(){return!0}function Ef(){return!1}function $n(e){function t(n,o,r,i,s){this._reactName=n,this._targetInst=r,this.type=o,this.nativeEvent=i,this.target=s,this.currentTarget=null;for(var l in e)e.hasOwnProperty(l)&&(n=e[l],this[l]=n?n(i):i[l]);return this.isDefaultPrevented=(i.defaultPrevented!=null?i.defaultPrevented:i.returnValue===!1)?Ma:Ef,this.isPropagationStopped=Ef,this}return Pt(t.prototype,{preventDefault:function(){this.defaultPrevented=!0;var n=this.nativeEvent;n&&(n.preventDefault?n.preventDefault():typeof n.returnValue!="unknown"&&(n.returnValue=!1),this.isDefaultPrevented=Ma)},stopPropagation:function(){var n=this.nativeEvent;n&&(n.stopPropagation?n.stopPropagation():typeof n.cancelBubble!="unknown"&&(n.cancelBubble=!0),this.isPropagationStopped=Ma)},persist:function(){},isPersistent:Ma}),t}var Xi={eventPhase:0,bubbles:0,cancelable:0,timeStamp:function(e){return e.timeStamp||Date.now()},defaultPrevented:0,isTrusted:0},E_=$n(Xi),xl=Pt({},Xi,{view:0,detail:0}),k2=$n(xl),id,sd,Fs,wc=Pt({},xl,{screenX:0,screenY:0,clientX:0,clientY:0,pageX:0,pageY:0,ctrlKey:0,shiftKey:0,altKey:0,metaKey:0,getModifierState:M_,button:0,buttons:0,relatedTarget:function(e){return e.relatedTarget===void 0?e.fromElement===e.srcElement?e.toElement:e.fromElement:e.relatedTarget},movementX:function(e){return"movementX"in e?e.movementX:(e!==Fs&&(Fs&&e.type==="mousemove"?(id=e.screenX-Fs.screenX,sd=e.screenY-Fs.screenY):sd=id=0,Fs=e),id)},movementY:function(e){return"movementY"in e?e.movementY:sd}}),Mf=$n(wc),S2=Pt({},wc,{dataTransfer:0}),C2=$n(S2),E2=Pt({},xl,{relatedTarget:0}),ld=$n(E2),M2=Pt({},Xi,{animationName:0,elapsedTime:0,pseudoElement:0}),T2=$n(M2),L2=Pt({},Xi,{clipboardData:function(e){return"clipboardData"in e?e.clipboardData:window.clipboardData}}),R2=$n(L2),P2=Pt({},Xi,{data:0}),Tf=$n(P2),N2={Esc:"Escape",Spacebar:" ",Left:"ArrowLeft",Up:"ArrowUp",Right:"ArrowRight",Down:"ArrowDown",Del:"Delete",Win:"OS",Menu:"ContextMenu",Apps:"ContextMenu",Scroll:"ScrollLock",MozPrintableKey:"Unidentified"},I2={8:"Backspace",9:"Tab",12:"Clear",13:"Enter",16:"Shift",17:"Control",18:"Alt",19:"Pause",20:"CapsLock",27:"Escape",32:" ",33:"PageUp",34:"PageDown",35:"End",36:"Home",37:"ArrowLeft",38:"ArrowUp",39:"ArrowRight",40:"ArrowDown",45:"Insert",46:"Delete",112:"F1",113:"F2",114:"F3",115:"F4",116:"F5",117:"F6",118:"F7",119:"F8",120:"F9",121:"F10",122:"F11",123:"F12",144:"NumLock",145:"ScrollLock",224:"Meta"},D2={Alt:"altKey",Control:"ctrlKey",Meta:"metaKey",Shift:"shiftKey"};function $2(e){var t=this.nativeEvent;return t.getModifierState?t.getModifierState(e):(e=D2[e])?!!t[e]:!1}function M_(){return $2}var A2=Pt({},xl,{key:function(e){if(e.key){var t=N2[e.key]||e.key;if(t!=="Unidentified")return t}return e.type==="keypress"?(e=Wa(e),e===13?"Enter":String.fromCharCode(e)):e.type==="keydown"||e.type==="keyup"?I2[e.keyCode]||"Unidentified":""},code:0,location:0,ctrlKey:0,shiftKey:0,altKey:0,metaKey:0,repeat:0,locale:0,getModifierState:M_,charCode:function(e){return e.type==="keypress"?Wa(e):0},keyCode:function(e){return e.type==="keydown"||e.type==="keyup"?e.keyCode:0},which:function(e){return e.type==="keypress"?Wa(e):e.type==="keydown"||e.type==="keyup"?e.keyCode:0}}),F2=$n(A2),O2=Pt({},wc,{pointerId:0,width:0,height:0,pressure:0,tangentialPressure:0,tiltX:0,tiltY:0,twist:0,pointerType:0,isPrimary:0}),Lf=$n(O2),B2=Pt({},xl,{touches:0,targetTouches:0,changedTouches:0,altKey:0,metaKey:0,ctrlKey:0,shiftKey:0,getModifierState:M_}),z2=$n(B2),H2=Pt({},Xi,{propertyName:0,elapsedTime:0,pseudoElement:0}),W2=$n(H2),U2=Pt({},wc,{deltaX:function(e){return"deltaX"in e?e.deltaX:"wheelDeltaX"in e?-e.wheelDeltaX:0},deltaY:function(e){return"deltaY"in e?e.deltaY:"wheelDeltaY"in e?-e.wheelDeltaY:"wheelDelta"in e?-e.wheelDelta:0},deltaZ:0,deltaMode:0}),j2=$n(U2),Y2=[9,13,27,32],T_=Wo&&"CompositionEvent"in window,Qs=null;Wo&&"documentMode"in document&&(Qs=document.documentMode);var V2=Wo&&"TextEvent"in window&&!Qs,Jg=Wo&&(!T_||Qs&&8<Qs&&11>=Qs),Rf=" ",Pf=!1;function Zg(e,t){switch(e){case"keyup":return Y2.indexOf(t.keyCode)!==-1;case"keydown":return t.keyCode!==229;case"keypress":case"mousedown":case"focusout":return!0;default:return!1}}function eh(e){return e=e.detail,typeof e=="object"&&"data"in e?e.data:null}var Si=!1;function X2(e,t){switch(e){case"compositionend":return eh(t);case"keypress":return t.which!==32?null:(Pf=!0,Rf);case"textInput":return e=t.data,e===Rf&&Pf?null:e;default:return null}}function Q2(e,t){if(Si)return e==="compositionend"||!T_&&Zg(e,t)?(e=Kg(),Ha=C_=ir=null,Si=!1,e):null;switch(e){case"paste":return null;case"keypress":if(!(t.ctrlKey||t.altKey||t.metaKey)||t.ctrlKey&&t.altKey){if(t.char&&1<t.char.length)return t.char;if(t.which)return String.fromCharCode(t.which)}return null;case"compositionend":return Jg&&t.locale!=="ko"?null:t.data;default:return null}}var G2={color:!0,date:!0,datetime:!0,"datetime-local":!0,email:!0,month:!0,number:!0,password:!0,range:!0,search:!0,tel:!0,text:!0,time:!0,url:!0,week:!0};function Nf(e){var t=e&&e.nodeName&&e.nodeName.toLowerCase();return t==="input"?!!G2[e.type]:t==="textarea"}function th(e,t,n,o){Ng(o),t=oc(t,"onChange"),0<t.length&&(n=new E_("onChange","change",null,n,o),e.push({event:n,listeners:t}))}var Gs=null,ll=null;function q2(e){_h(e,0)}function bc(e){var t=Mi(e);if(Cg(t))return e}function K2(e,t){if(e==="change")return t}var nh=!1;Wo&&(Wo?(La="oninput"in document,La||(ad=document.createElement("div"),ad.setAttribute("oninput","return;"),La=typeof ad.oninput=="function"),Ta=La):Ta=!1,nh=Ta&&(!document.documentMode||9<document.documentMode));var Ta,La,ad;function If(){Gs&&(Gs.detachEvent("onpropertychange",oh),ll=Gs=null)}function oh(e){if(e.propertyName==="value"&&bc(ll)){var t=[];th(t,ll,e,v_(e)),Ag(q2,t)}}function J2(e,t,n){e==="focusin"?(If(),Gs=t,ll=n,Gs.attachEvent("onpropertychange",oh)):e==="focusout"&&If()}function Z2(e){if(e==="selectionchange"||e==="keyup"||e==="keydown")return bc(ll)}function ex(e,t){if(e==="click")return bc(t)}function tx(e,t){if(e==="input"||e==="change")return bc(t)}function nx(e,t){return e===t&&(e!==0||1/e===1/t)||e!==e&&t!==t}var mo=typeof Object.is=="function"?Object.is:nx;function al(e,t){if(mo(e,t))return!0;if(typeof e!="object"||e===null||typeof t!="object"||t===null)return!1;var n=Object.keys(e),o=Object.keys(t);if(n.length!==o.length)return!1;for(o=0;o<n.length;o++){var r=n[o];if(!bd.call(t,r)||!mo(e[r],t[r]))return!1}return!0}function Df(e){for(;e&&e.firstChild;)e=e.firstChild;return e}function $f(e,t){var n=Df(e);e=0;for(var o;n;){if(n.nodeType===3){if(o=e+n.textContent.length,e<=t&&o>=t)return{node:n,offset:t-e};e=o}e:{for(;n;){if(n.nextSibling){n=n.nextSibling;break e}n=n.parentNode}n=void 0}n=Df(n)}}function rh(e,t){return e&&t?e===t?!0:e&&e.nodeType===3?!1:t&&t.nodeType===3?rh(e,t.parentNode):"contains"in e?e.contains(t):e.compareDocumentPosition?!!(e.compareDocumentPosition(t)&16):!1:!1}function ih(){for(var e=window,t=qa();t instanceof e.HTMLIFrameElement;){try{var n=typeof t.contentWindow.location.href=="string"}catch{n=!1}if(n)e=t.contentWindow;else break;t=qa(e.document)}return t}function L_(e){var t=e&&e.nodeName&&e.nodeName.toLowerCase();return t&&(t==="input"&&(e.type==="text"||e.type==="search"||e.type==="tel"||e.type==="url"||e.type==="password")||t==="textarea"||e.contentEditable==="true")}function ox(e){var t=ih(),n=e.focusedElem,o=e.selectionRange;if(t!==n&&n&&n.ownerDocument&&rh(n.ownerDocument.documentElement,n)){if(o!==null&&L_(n)){if(t=o.start,e=o.end,e===void 0&&(e=t),"selectionStart"in n)n.selectionStart=t,n.selectionEnd=Math.min(e,n.value.length);else if(e=(t=n.ownerDocument||document)&&t.defaultView||window,e.getSelection){e=e.getSelection();var r=n.textContent.length,i=Math.min(o.start,r);o=o.end===void 0?i:Math.min(o.end,r),!e.extend&&i>o&&(r=o,o=i,i=r),r=$f(n,i);var s=$f(n,o);r&&s&&(e.rangeCount!==1||e.anchorNode!==r.node||e.anchorOffset!==r.offset||e.focusNode!==s.node||e.focusOffset!==s.offset)&&(t=t.createRange(),t.setStart(r.node,r.offset),e.removeAllRanges(),i>o?(e.addRange(t),e.extend(s.node,s.offset)):(t.setEnd(s.node,s.offset),e.addRange(t)))}}for(t=[],e=n;e=e.parentNode;)e.nodeType===1&&t.push({element:e,left:e.scrollLeft,top:e.scrollTop});for(typeof n.focus=="function"&&n.focus(),n=0;n<t.length;n++)e=t[n],e.element.scrollLeft=e.left,e.element.scrollTop=e.top}}var rx=Wo&&"documentMode"in document&&11>=document.documentMode,Ci=null,Hd=null,qs=null,Wd=!1;function Af(e,t,n){var o=n.window===n?n.document:n.nodeType===9?n:n.ownerDocument;Wd||Ci==null||Ci!==qa(o)||(o=Ci,"selectionStart"in o&&L_(o)?o={start:o.selectionStart,end:o.selectionEnd}:(o=(o.ownerDocument&&o.ownerDocument.defaultView||window).getSelection(),o={anchorNode:o.anchorNode,anchorOffset:o.anchorOffset,focusNode:o.focusNode,focusOffset:o.focusOffset}),qs&&al(qs,o)||(qs=o,o=oc(Hd,"onSelect"),0<o.length&&(t=new E_("onSelect","select",null,t,n),e.push({event:t,listeners:o}),t.target=Ci)))}function Ra(e,t){var n={};return n[e.toLowerCase()]=t.toLowerCase(),n["Webkit"+e]="webkit"+t,n["Moz"+e]="moz"+t,n}var Ei={animationend:Ra("Animation","AnimationEnd"),animationiteration:Ra("Animation","AnimationIteration"),animationstart:Ra("Animation","AnimationStart"),transitionend:Ra("Transition","TransitionEnd")},cd={},sh={};Wo&&(sh=document.createElement("div").style,"AnimationEvent"in window||(delete Ei.animationend.animation,delete Ei.animationiteration.animation,delete Ei.animationstart.animation),"TransitionEvent"in window||delete Ei.transitionend.transition);function kc(e){if(cd[e])return cd[e];if(!Ei[e])return e;var t=Ei[e],n;for(n in t)if(t.hasOwnProperty(n)&&n in sh)return cd[e]=t[n];return e}var lh=kc("animationend"),ah=kc("animationiteration"),ch=kc("animationstart"),uh=kc("transitionend"),dh=new Map,Ff="abort auxClick cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");function hr(e,t){dh.set(e,t),ni(t,[e])}for(Pa=0;Pa<Ff.length;Pa++)Na=Ff[Pa],Of=Na.toLowerCase(),Bf=Na[0].toUpperCase()+Na.slice(1),hr(Of,"on"+Bf);var Na,Of,Bf,Pa;hr(lh,"onAnimationEnd");hr(ah,"onAnimationIteration");hr(ch,"onAnimationStart");hr("dblclick","onDoubleClick");hr("focusin","onFocus");hr("focusout","onBlur");hr(uh,"onTransitionEnd");zi("onMouseEnter",["mouseout","mouseover"]);zi("onMouseLeave",["mouseout","mouseover"]);zi("onPointerEnter",["pointerout","pointerover"]);zi("onPointerLeave",["pointerout","pointerover"]);ni("onChange","change click focusin focusout input keydown keyup selectionchange".split(" "));ni("onSelect","focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" "));ni("onBeforeInput",["compositionend","keypress","textInput","paste"]);ni("onCompositionEnd","compositionend focusout keydown keypress keyup mousedown".split(" "));ni("onCompositionStart","compositionstart focusout keydown keypress keyup mousedown".split(" "));ni("onCompositionUpdate","compositionupdate focusout keydown keypress keyup mousedown".split(" "));var Ys="abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "),ix=new Set("cancel close invalid load scroll toggle".split(" ").concat(Ys));function zf(e,t,n){var o=e.type||"unknown-event";e.currentTarget=n,i2(o,t,void 0,e),e.currentTarget=null}function _h(e,t){t=(t&4)!==0;for(var n=0;n<e.length;n++){var o=e[n],r=o.event;o=o.listeners;e:{var i=void 0;if(t)for(var s=o.length-1;0<=s;s--){var l=o[s],a=l.instance,c=l.currentTarget;if(l=l.listener,a!==i&&r.isPropagationStopped())break e;zf(r,l,c),i=a}else for(s=0;s<o.length;s++){if(l=o[s],a=l.instance,c=l.currentTarget,l=l.listener,a!==i&&r.isPropagationStopped())break e;zf(r,l,c),i=a}}}if(Ja)throw e=Fd,Ja=!1,Fd=null,e}function St(e,t){var n=t[Xd];n===void 0&&(n=t[Xd]=new Set);var o=e+"__bubble";n.has(o)||(mh(t,e,2,!1),n.add(o))}function ud(e,t,n){var o=0;t&&(o|=4),mh(n,e,o,t)}var Ia="_reactListening"+Math.random().toString(36).slice(2);function cl(e){if(!e[Ia]){e[Ia]=!0,vg.forEach(function(n){n!=="selectionchange"&&(ix.has(n)||ud(n,!1,e),ud(n,!0,e))});var t=e.nodeType===9?e:e.ownerDocument;t===null||t[Ia]||(t[Ia]=!0,ud("selectionchange",!1,t))}}function mh(e,t,n,o){switch(qg(t)){case 1:var r=w2;break;case 4:r=b2;break;default:r=S_}n=r.bind(null,t,n,e),r=void 0,!Ad||t!=="touchstart"&&t!=="touchmove"&&t!=="wheel"||(r=!0),o?r!==void 0?e.addEventListener(t,n,{capture:!0,passive:r}):e.addEventListener(t,n,!0):r!==void 0?e.addEventListener(t,n,{passive:r}):e.addEventListener(t,n,!1)}function dd(e,t,n,o,r){var i=o;if((t&1)===0&&(t&2)===0&&o!==null)e:for(;;){if(o===null)return;var s=o.tag;if(s===3||s===4){var l=o.stateNode.containerInfo;if(l===r||l.nodeType===8&&l.parentNode===r)break;if(s===4)for(s=o.return;s!==null;){var a=s.tag;if((a===3||a===4)&&(a=s.stateNode.containerInfo,a===r||a.nodeType===8&&a.parentNode===r))return;s=s.return}for(;l!==null;){if(s=Vr(l),s===null)return;if(a=s.tag,a===5||a===6){o=i=s;continue e}l=l.parentNode}}o=o.return}Ag(function(){var c=i,d=v_(n),m=[];e:{var f=dh.get(e);if(f!==void 0){var k=E_,w=e;switch(e){case"keypress":if(Wa(n)===0)break e;case"keydown":case"keyup":k=F2;break;case"focusin":w="focus",k=ld;break;case"focusout":w="blur",k=ld;break;case"beforeblur":case"afterblur":k=ld;break;case"click":if(n.button===2)break e;case"auxclick":case"dblclick":case"mousedown":case"mousemove":case"mouseup":case"mouseout":case"mouseover":case"contextmenu":k=Mf;break;case"drag":case"dragend":case"dragenter":case"dragexit":case"dragleave":case"dragover":case"dragstart":case"drop":k=C2;break;case"touchcancel":case"touchend":case"touchmove":case"touchstart":k=z2;break;case lh:case ah:case ch:k=T2;break;case uh:k=W2;break;case"scroll":k=k2;break;case"wheel":k=j2;break;case"copy":case"cut":case"paste":k=R2;break;case"gotpointercapture":case"lostpointercapture":case"pointercancel":case"pointerdown":case"pointermove":case"pointerout":case"pointerover":case"pointerup":k=Lf}var C=(t&4)!==0,S=!C&&e==="scroll",g=C?f!==null?f+"Capture":null:f;C=[];for(var x=c,h;x!==null;){h=x;var E=h.stateNode;if(h.tag===5&&E!==null&&(h=E,g!==null&&(E=ol(x,g),E!=null&&C.push(ul(x,E,h)))),S)break;x=x.return}0<C.length&&(f=new k(f,w,null,n,d),m.push({event:f,listeners:C}))}}if((t&7)===0){e:{if(f=e==="mouseover"||e==="pointerover",k=e==="mouseout"||e==="pointerout",f&&n!==Dd&&(w=n.relatedTarget||n.fromElement)&&(Vr(w)||w[Uo]))break e;if((k||f)&&(f=d.window===d?d:(f=d.ownerDocument)?f.defaultView||f.parentWindow:window,k?(w=n.relatedTarget||n.toElement,k=c,w=w?Vr(w):null,w!==null&&(S=oi(w),w!==S||w.tag!==5&&w.tag!==6)&&(w=null)):(k=null,w=c),k!==w)){if(C=Mf,E="onMouseLeave",g="onMouseEnter",x="mouse",(e==="pointerout"||e==="pointerover")&&(C=Lf,E="onPointerLeave",g="onPointerEnter",x="pointer"),S=k==null?f:Mi(k),h=w==null?f:Mi(w),f=new C(E,x+"leave",k,n,d),f.target=S,f.relatedTarget=h,E=null,Vr(d)===c&&(C=new C(g,x+"enter",w,n,d),C.target=h,C.relatedTarget=S,E=C),S=E,k&&w)t:{for(C=k,g=w,x=0,h=C;h;h=wi(h))x++;for(h=0,E=g;E;E=wi(E))h++;for(;0<x-h;)C=wi(C),x--;for(;0<h-x;)g=wi(g),h--;for(;x--;){if(C===g||g!==null&&C===g.alternate)break t;C=wi(C),g=wi(g)}C=null}else C=null;k!==null&&Hf(m,f,k,C,!1),w!==null&&S!==null&&Hf(m,S,w,C,!0)}}e:{if(f=c?Mi(c):window,k=f.nodeName&&f.nodeName.toLowerCase(),k==="select"||k==="input"&&f.type==="file")var F=K2;else if(Nf(f))if(nh)F=tx;else{F=Z2;var W=J2}else(k=f.nodeName)&&k.toLowerCase()==="input"&&(f.type==="checkbox"||f.type==="radio")&&(F=ex);if(F&&(F=F(e,c))){th(m,F,n,d);break e}W&&W(e,f,c),e==="focusout"&&(W=f._wrapperState)&&W.controlled&&f.type==="number"&&Ld(f,"number",f.value)}switch(W=c?Mi(c):window,e){case"focusin":(Nf(W)||W.contentEditable==="true")&&(Ci=W,Hd=c,qs=null);break;case"focusout":qs=Hd=Ci=null;break;case"mousedown":Wd=!0;break;case"contextmenu":case"mouseup":case"dragend":Wd=!1,Af(m,n,d);break;case"selectionchange":if(rx)break;case"keydown":case"keyup":Af(m,n,d)}var D;if(T_)e:{switch(e){case"compositionstart":var j="onCompositionStart";break e;case"compositionend":j="onCompositionEnd";break e;case"compositionupdate":j="onCompositionUpdate";break e}j=void 0}else Si?Zg(e,n)&&(j="onCompositionEnd"):e==="keydown"&&n.keyCode===229&&(j="onCompositionStart");j&&(Jg&&n.locale!=="ko"&&(Si||j!=="onCompositionStart"?j==="onCompositionEnd"&&Si&&(D=Kg()):(ir=d,C_="value"in ir?ir.value:ir.textContent,Si=!0)),W=oc(c,j),0<W.length&&(j=new Tf(j,e,null,n,d),m.push({event:j,listeners:W}),D?j.data=D:(D=eh(n),D!==null&&(j.data=D)))),(D=V2?X2(e,n):Q2(e,n))&&(c=oc(c,"onBeforeInput"),0<c.length&&(d=new Tf("onBeforeInput","beforeinput",null,n,d),m.push({event:d,listeners:c}),d.data=D))}_h(m,t)})}function ul(e,t,n){return{instance:e,listener:t,currentTarget:n}}function oc(e,t){for(var n=t+"Capture",o=[];e!==null;){var r=e,i=r.stateNode;r.tag===5&&i!==null&&(r=i,i=ol(e,n),i!=null&&o.unshift(ul(e,i,r)),i=ol(e,t),i!=null&&o.push(ul(e,i,r))),e=e.return}return o}function wi(e){if(e===null)return null;do e=e.return;while(e&&e.tag!==5);return e||null}function Hf(e,t,n,o,r){for(var i=t._reactName,s=[];n!==null&&n!==o;){var l=n,a=l.alternate,c=l.stateNode;if(a!==null&&a===o)break;l.tag===5&&c!==null&&(l=c,r?(a=ol(n,i),a!=null&&s.unshift(ul(n,a,l))):r||(a=ol(n,i),a!=null&&s.push(ul(n,a,l)))),n=n.return}s.length!==0&&e.push({event:t,listeners:s})}var sx=/\r\n?/g,lx=/\u0000|\uFFFD/g;function Wf(e){return(typeof e=="string"?e:""+e).replace(sx,`
`).replace(lx,"")}function Da(e,t,n){if(t=Wf(t),Wf(e)!==t&&n)throw Error(Q(425))}function rc(){}var Ud=null,jd=null;function Yd(e,t){return e==="textarea"||e==="noscript"||typeof t.children=="string"||typeof t.children=="number"||typeof t.dangerouslySetInnerHTML=="object"&&t.dangerouslySetInnerHTML!==null&&t.dangerouslySetInnerHTML.__html!=null}var Vd=typeof setTimeout=="function"?setTimeout:void 0,ax=typeof clearTimeout=="function"?clearTimeout:void 0,Uf=typeof Promise=="function"?Promise:void 0,cx=typeof queueMicrotask=="function"?queueMicrotask:typeof Uf<"u"?function(e){return Uf.resolve(null).then(e).catch(ux)}:Vd;function ux(e){setTimeout(function(){throw e})}function _d(e,t){var n=t,o=0;do{var r=n.nextSibling;if(e.removeChild(n),r&&r.nodeType===8)if(n=r.data,n==="/$"){if(o===0){e.removeChild(r),sl(t);return}o--}else n!=="$"&&n!=="$?"&&n!=="$!"||o++;n=r}while(n);sl(t)}function ur(e){for(;e!=null;e=e.nextSibling){var t=e.nodeType;if(t===1||t===3)break;if(t===8){if(t=e.data,t==="$"||t==="$!"||t==="$?")break;if(t==="/$")return null}}return e}function jf(e){e=e.previousSibling;for(var t=0;e;){if(e.nodeType===8){var n=e.data;if(n==="$"||n==="$!"||n==="$?"){if(t===0)return e;t--}else n==="/$"&&t++}e=e.previousSibling}return null}var Qi=Math.random().toString(36).slice(2),Eo="__reactFiber$"+Qi,dl="__reactProps$"+Qi,Uo="__reactContainer$"+Qi,Xd="__reactEvents$"+Qi,dx="__reactListeners$"+Qi,_x="__reactHandles$"+Qi;function Vr(e){var t=e[Eo];if(t)return t;for(var n=e.parentNode;n;){if(t=n[Uo]||n[Eo]){if(n=t.alternate,t.child!==null||n!==null&&n.child!==null)for(e=jf(e);e!==null;){if(n=e[Eo])return n;e=jf(e)}return t}e=n,n=e.parentNode}return null}function vl(e){return e=e[Eo]||e[Uo],!e||e.tag!==5&&e.tag!==6&&e.tag!==13&&e.tag!==3?null:e}function Mi(e){if(e.tag===5||e.tag===6)return e.stateNode;throw Error(Q(33))}function Sc(e){return e[dl]||null}var Qd=[],Ti=-1;function yr(e){return{current:e}}function Ct(e){0>Ti||(e.current=Qd[Ti],Qd[Ti]=null,Ti--)}function bt(e,t){Ti++,Qd[Ti]=e.current,e.current=t}var gr={},un=yr(gr),Cn=yr(!1),Kr=gr;function Hi(e,t){var n=e.type.contextTypes;if(!n)return gr;var o=e.stateNode;if(o&&o.__reactInternalMemoizedUnmaskedChildContext===t)return o.__reactInternalMemoizedMaskedChildContext;var r={},i;for(i in n)r[i]=t[i];return o&&(e=e.stateNode,e.__reactInternalMemoizedUnmaskedChildContext=t,e.__reactInternalMemoizedMaskedChildContext=r),r}function En(e){return e=e.childContextTypes,e!=null}function ic(){Ct(Cn),Ct(un)}function Yf(e,t,n){if(un.current!==gr)throw Error(Q(168));bt(un,t),bt(Cn,n)}function ph(e,t,n){var o=e.stateNode;if(t=t.childContextTypes,typeof o.getChildContext!="function")return n;o=o.getChildContext();for(var r in o)if(!(r in t))throw Error(Q(108,J5(e)||"Unknown",r));return Pt({},n,o)}function sc(e){return e=(e=e.stateNode)&&e.__reactInternalMemoizedMergedChildContext||gr,Kr=un.current,bt(un,e),bt(Cn,Cn.current),!0}function Vf(e,t,n){var o=e.stateNode;if(!o)throw Error(Q(169));n?(e=ph(e,t,Kr),o.__reactInternalMemoizedMergedChildContext=e,Ct(Cn),Ct(un),bt(un,e)):Ct(Cn),bt(Cn,n)}var Oo=null,Cc=!1,md=!1;function fh(e){Oo===null?Oo=[e]:Oo.push(e)}function mx(e){Cc=!0,fh(e)}function xr(){if(!md&&Oo!==null){md=!0;var e=0,t=mt;try{var n=Oo;for(mt=1;e<n.length;e++){var o=n[e];do o=o(!0);while(o!==null)}Oo=null,Cc=!1}catch(r){throw Oo!==null&&(Oo=Oo.slice(e+1)),zg(w_,xr),r}finally{mt=t,md=!1}}return null}var Li=[],Ri=0,lc=null,ac=0,jn=[],Yn=0,Jr=null,Bo=1,zo="";function jr(e,t){Li[Ri++]=ac,Li[Ri++]=lc,lc=e,ac=t}function gh(e,t,n){jn[Yn++]=Bo,jn[Yn++]=zo,jn[Yn++]=Jr,Jr=e;var o=Bo;e=zo;var r=32-uo(o)-1;o&=~(1<<r),n+=1;var i=32-uo(t)+r;if(30<i){var s=r-r%5;i=(o&(1<<s)-1).toString(32),o>>=s,r-=s,Bo=1<<32-uo(t)+r|n<<r|o,zo=i+e}else Bo=1<<i|n<<r|o,zo=e}function R_(e){e.return!==null&&(jr(e,1),gh(e,1,0))}function P_(e){for(;e===lc;)lc=Li[--Ri],Li[Ri]=null,ac=Li[--Ri],Li[Ri]=null;for(;e===Jr;)Jr=jn[--Yn],jn[Yn]=null,zo=jn[--Yn],jn[Yn]=null,Bo=jn[--Yn],jn[Yn]=null}var In=null,Nn=null,Tt=!1,co=null;function hh(e,t){var n=Vn(5,null,null,0);n.elementType="DELETED",n.stateNode=t,n.return=e,t=e.deletions,t===null?(e.deletions=[n],e.flags|=16):t.push(n)}function Xf(e,t){switch(e.tag){case 5:var n=e.type;return t=t.nodeType!==1||n.toLowerCase()!==t.nodeName.toLowerCase()?null:t,t!==null?(e.stateNode=t,In=e,Nn=ur(t.firstChild),!0):!1;case 6:return t=e.pendingProps===""||t.nodeType!==3?null:t,t!==null?(e.stateNode=t,In=e,Nn=null,!0):!1;case 13:return t=t.nodeType!==8?null:t,t!==null?(n=Jr!==null?{id:Bo,overflow:zo}:null,e.memoizedState={dehydrated:t,treeContext:n,retryLane:1073741824},n=Vn(18,null,null,0),n.stateNode=t,n.return=e,e.child=n,In=e,Nn=null,!0):!1;default:return!1}}function Gd(e){return(e.mode&1)!==0&&(e.flags&128)===0}function qd(e){if(Tt){var t=Nn;if(t){var n=t;if(!Xf(e,t)){if(Gd(e))throw Error(Q(418));t=ur(n.nextSibling);var o=In;t&&Xf(e,t)?hh(o,n):(e.flags=e.flags&-4097|2,Tt=!1,In=e)}}else{if(Gd(e))throw Error(Q(418));e.flags=e.flags&-4097|2,Tt=!1,In=e}}}function Qf(e){for(e=e.return;e!==null&&e.tag!==5&&e.tag!==3&&e.tag!==13;)e=e.return;In=e}function $a(e){if(e!==In)return!1;if(!Tt)return Qf(e),Tt=!0,!1;var t;if((t=e.tag!==3)&&!(t=e.tag!==5)&&(t=e.type,t=t!=="head"&&t!=="body"&&!Yd(e.type,e.memoizedProps)),t&&(t=Nn)){if(Gd(e))throw yh(),Error(Q(418));for(;t;)hh(e,t),t=ur(t.nextSibling)}if(Qf(e),e.tag===13){if(e=e.memoizedState,e=e!==null?e.dehydrated:null,!e)throw Error(Q(317));e:{for(e=e.nextSibling,t=0;e;){if(e.nodeType===8){var n=e.data;if(n==="/$"){if(t===0){Nn=ur(e.nextSibling);break e}t--}else n!=="$"&&n!=="$!"&&n!=="$?"||t++}e=e.nextSibling}Nn=null}}else Nn=In?ur(e.stateNode.nextSibling):null;return!0}function yh(){for(var e=Nn;e;)e=ur(e.nextSibling)}function Wi(){Nn=In=null,Tt=!1}function N_(e){co===null?co=[e]:co.push(e)}var px=Vo.ReactCurrentBatchConfig;function Os(e,t,n){if(e=n.ref,e!==null&&typeof e!="function"&&typeof e!="object"){if(n._owner){if(n=n._owner,n){if(n.tag!==1)throw Error(Q(309));var o=n.stateNode}if(!o)throw Error(Q(147,e));var r=o,i=""+e;return t!==null&&t.ref!==null&&typeof t.ref=="function"&&t.ref._stringRef===i?t.ref:(t=function(s){var l=r.refs;s===null?delete l[i]:l[i]=s},t._stringRef=i,t)}if(typeof e!="string")throw Error(Q(284));if(!n._owner)throw Error(Q(290,e))}return e}function Aa(e,t){throw e=Object.prototype.toString.call(t),Error(Q(31,e==="[object Object]"?"object with keys {"+Object.keys(t).join(", ")+"}":e))}function Gf(e){var t=e._init;return t(e._payload)}function xh(e){function t(g,x){if(e){var h=g.deletions;h===null?(g.deletions=[x],g.flags|=16):h.push(x)}}function n(g,x){if(!e)return null;for(;x!==null;)t(g,x),x=x.sibling;return null}function o(g,x){for(g=new Map;x!==null;)x.key!==null?g.set(x.key,x):g.set(x.index,x),x=x.sibling;return g}function r(g,x){return g=pr(g,x),g.index=0,g.sibling=null,g}function i(g,x,h){return g.index=h,e?(h=g.alternate,h!==null?(h=h.index,h<x?(g.flags|=2,x):h):(g.flags|=2,x)):(g.flags|=1048576,x)}function s(g){return e&&g.alternate===null&&(g.flags|=2),g}function l(g,x,h,E){return x===null||x.tag!==6?(x=vd(h,g.mode,E),x.return=g,x):(x=r(x,h),x.return=g,x)}function a(g,x,h,E){var F=h.type;return F===ki?d(g,x,h.props.children,E,h.key):x!==null&&(x.elementType===F||typeof F=="object"&&F!==null&&F.$$typeof===tr&&Gf(F)===x.type)?(E=r(x,h.props),E.ref=Os(g,x,h),E.return=g,E):(E=Ga(h.type,h.key,h.props,null,g.mode,E),E.ref=Os(g,x,h),E.return=g,E)}function c(g,x,h,E){return x===null||x.tag!==4||x.stateNode.containerInfo!==h.containerInfo||x.stateNode.implementation!==h.implementation?(x=wd(h,g.mode,E),x.return=g,x):(x=r(x,h.children||[]),x.return=g,x)}function d(g,x,h,E,F){return x===null||x.tag!==7?(x=qr(h,g.mode,E,F),x.return=g,x):(x=r(x,h),x.return=g,x)}function m(g,x,h){if(typeof x=="string"&&x!==""||typeof x=="number")return x=vd(""+x,g.mode,h),x.return=g,x;if(typeof x=="object"&&x!==null){switch(x.$$typeof){case wa:return h=Ga(x.type,x.key,x.props,null,g.mode,h),h.ref=Os(g,null,x),h.return=g,h;case bi:return x=wd(x,g.mode,h),x.return=g,x;case tr:var E=x._init;return m(g,E(x._payload),h)}if(Us(x)||Ds(x))return x=qr(x,g.mode,h,null),x.return=g,x;Aa(g,x)}return null}function f(g,x,h,E){var F=x!==null?x.key:null;if(typeof h=="string"&&h!==""||typeof h=="number")return F!==null?null:l(g,x,""+h,E);if(typeof h=="object"&&h!==null){switch(h.$$typeof){case wa:return h.key===F?a(g,x,h,E):null;case bi:return h.key===F?c(g,x,h,E):null;case tr:return F=h._init,f(g,x,F(h._payload),E)}if(Us(h)||Ds(h))return F!==null?null:d(g,x,h,E,null);Aa(g,h)}return null}function k(g,x,h,E,F){if(typeof E=="string"&&E!==""||typeof E=="number")return g=g.get(h)||null,l(x,g,""+E,F);if(typeof E=="object"&&E!==null){switch(E.$$typeof){case wa:return g=g.get(E.key===null?h:E.key)||null,a(x,g,E,F);case bi:return g=g.get(E.key===null?h:E.key)||null,c(x,g,E,F);case tr:var W=E._init;return k(g,x,h,W(E._payload),F)}if(Us(E)||Ds(E))return g=g.get(h)||null,d(x,g,E,F,null);Aa(x,E)}return null}function w(g,x,h,E){for(var F=null,W=null,D=x,j=x=0,Z=null;D!==null&&j<h.length;j++){D.index>j?(Z=D,D=null):Z=D.sibling;var q=f(g,D,h[j],E);if(q===null){D===null&&(D=Z);break}e&&D&&q.alternate===null&&t(g,D),x=i(q,x,j),W===null?F=q:W.sibling=q,W=q,D=Z}if(j===h.length)return n(g,D),Tt&&jr(g,j),F;if(D===null){for(;j<h.length;j++)D=m(g,h[j],E),D!==null&&(x=i(D,x,j),W===null?F=D:W.sibling=D,W=D);return Tt&&jr(g,j),F}for(D=o(g,D);j<h.length;j++)Z=k(D,g,j,h[j],E),Z!==null&&(e&&Z.alternate!==null&&D.delete(Z.key===null?j:Z.key),x=i(Z,x,j),W===null?F=Z:W.sibling=Z,W=Z);return e&&D.forEach(function(O){return t(g,O)}),Tt&&jr(g,j),F}function C(g,x,h,E){var F=Ds(h);if(typeof F!="function")throw Error(Q(150));if(h=F.call(h),h==null)throw Error(Q(151));for(var W=F=null,D=x,j=x=0,Z=null,q=h.next();D!==null&&!q.done;j++,q=h.next()){D.index>j?(Z=D,D=null):Z=D.sibling;var O=f(g,D,q.value,E);if(O===null){D===null&&(D=Z);break}e&&D&&O.alternate===null&&t(g,D),x=i(O,x,j),W===null?F=O:W.sibling=O,W=O,D=Z}if(q.done)return n(g,D),Tt&&jr(g,j),F;if(D===null){for(;!q.done;j++,q=h.next())q=m(g,q.value,E),q!==null&&(x=i(q,x,j),W===null?F=q:W.sibling=q,W=q);return Tt&&jr(g,j),F}for(D=o(g,D);!q.done;j++,q=h.next())q=k(D,g,j,q.value,E),q!==null&&(e&&q.alternate!==null&&D.delete(q.key===null?j:q.key),x=i(q,x,j),W===null?F=q:W.sibling=q,W=q);return e&&D.forEach(function(Ae){return t(g,Ae)}),Tt&&jr(g,j),F}function S(g,x,h,E){if(typeof h=="object"&&h!==null&&h.type===ki&&h.key===null&&(h=h.props.children),typeof h=="object"&&h!==null){switch(h.$$typeof){case wa:e:{for(var F=h.key,W=x;W!==null;){if(W.key===F){if(F=h.type,F===ki){if(W.tag===7){n(g,W.sibling),x=r(W,h.props.children),x.return=g,g=x;break e}}else if(W.elementType===F||typeof F=="object"&&F!==null&&F.$$typeof===tr&&Gf(F)===W.type){n(g,W.sibling),x=r(W,h.props),x.ref=Os(g,W,h),x.return=g,g=x;break e}n(g,W);break}else t(g,W);W=W.sibling}h.type===ki?(x=qr(h.props.children,g.mode,E,h.key),x.return=g,g=x):(E=Ga(h.type,h.key,h.props,null,g.mode,E),E.ref=Os(g,x,h),E.return=g,g=E)}return s(g);case bi:e:{for(W=h.key;x!==null;){if(x.key===W)if(x.tag===4&&x.stateNode.containerInfo===h.containerInfo&&x.stateNode.implementation===h.implementation){n(g,x.sibling),x=r(x,h.children||[]),x.return=g,g=x;break e}else{n(g,x);break}else t(g,x);x=x.sibling}x=wd(h,g.mode,E),x.return=g,g=x}return s(g);case tr:return W=h._init,S(g,x,W(h._payload),E)}if(Us(h))return w(g,x,h,E);if(Ds(h))return C(g,x,h,E);Aa(g,h)}return typeof h=="string"&&h!==""||typeof h=="number"?(h=""+h,x!==null&&x.tag===6?(n(g,x.sibling),x=r(x,h),x.return=g,g=x):(n(g,x),x=vd(h,g.mode,E),x.return=g,g=x),s(g)):n(g,x)}return S}var Ui=xh(!0),vh=xh(!1),cc=yr(null),uc=null,Pi=null,I_=null;function D_(){I_=Pi=uc=null}function $_(e){var t=cc.current;Ct(cc),e._currentValue=t}function Kd(e,t,n){for(;e!==null;){var o=e.alternate;if((e.childLanes&t)!==t?(e.childLanes|=t,o!==null&&(o.childLanes|=t)):o!==null&&(o.childLanes&t)!==t&&(o.childLanes|=t),e===n)break;e=e.return}}function Oi(e,t){uc=e,I_=Pi=null,e=e.dependencies,e!==null&&e.firstContext!==null&&((e.lanes&t)!==0&&(Sn=!0),e.firstContext=null)}function Qn(e){var t=e._currentValue;if(I_!==e)if(e={context:e,memoizedValue:t,next:null},Pi===null){if(uc===null)throw Error(Q(308));Pi=e,uc.dependencies={lanes:0,firstContext:e}}else Pi=Pi.next=e;return t}var Xr=null;function A_(e){Xr===null?Xr=[e]:Xr.push(e)}function wh(e,t,n,o){var r=t.interleaved;return r===null?(n.next=n,A_(t)):(n.next=r.next,r.next=n),t.interleaved=n,jo(e,o)}function jo(e,t){e.lanes|=t;var n=e.alternate;for(n!==null&&(n.lanes|=t),n=e,e=e.return;e!==null;)e.childLanes|=t,n=e.alternate,n!==null&&(n.childLanes|=t),n=e,e=e.return;return n.tag===3?n.stateNode:null}var nr=!1;function F_(e){e.updateQueue={baseState:e.memoizedState,firstBaseUpdate:null,lastBaseUpdate:null,shared:{pending:null,interleaved:null,lanes:0},effects:null}}function bh(e,t){e=e.updateQueue,t.updateQueue===e&&(t.updateQueue={baseState:e.baseState,firstBaseUpdate:e.firstBaseUpdate,lastBaseUpdate:e.lastBaseUpdate,shared:e.shared,effects:e.effects})}function Ho(e,t){return{eventTime:e,lane:t,tag:0,payload:null,callback:null,next:null}}function dr(e,t,n){var o=e.updateQueue;if(o===null)return null;if(o=o.shared,(Je&2)!==0){var r=o.pending;return r===null?t.next=t:(t.next=r.next,r.next=t),o.pending=t,jo(e,n)}return r=o.interleaved,r===null?(t.next=t,A_(o)):(t.next=r.next,r.next=t),o.interleaved=t,jo(e,n)}function Ua(e,t,n){if(t=t.updateQueue,t!==null&&(t=t.shared,(n&4194240)!==0)){var o=t.lanes;o&=e.pendingLanes,n|=o,t.lanes=n,b_(e,n)}}function qf(e,t){var n=e.updateQueue,o=e.alternate;if(o!==null&&(o=o.updateQueue,n===o)){var r=null,i=null;if(n=n.firstBaseUpdate,n!==null){do{var s={eventTime:n.eventTime,lane:n.lane,tag:n.tag,payload:n.payload,callback:n.callback,next:null};i===null?r=i=s:i=i.next=s,n=n.next}while(n!==null);i===null?r=i=t:i=i.next=t}else r=i=t;n={baseState:o.baseState,firstBaseUpdate:r,lastBaseUpdate:i,shared:o.shared,effects:o.effects},e.updateQueue=n;return}e=n.lastBaseUpdate,e===null?n.firstBaseUpdate=t:e.next=t,n.lastBaseUpdate=t}function dc(e,t,n,o){var r=e.updateQueue;nr=!1;var i=r.firstBaseUpdate,s=r.lastBaseUpdate,l=r.shared.pending;if(l!==null){r.shared.pending=null;var a=l,c=a.next;a.next=null,s===null?i=c:s.next=c,s=a;var d=e.alternate;d!==null&&(d=d.updateQueue,l=d.lastBaseUpdate,l!==s&&(l===null?d.firstBaseUpdate=c:l.next=c,d.lastBaseUpdate=a))}if(i!==null){var m=r.baseState;s=0,d=c=a=null,l=i;do{var f=l.lane,k=l.eventTime;if((o&f)===f){d!==null&&(d=d.next={eventTime:k,lane:0,tag:l.tag,payload:l.payload,callback:l.callback,next:null});e:{var w=e,C=l;switch(f=t,k=n,C.tag){case 1:if(w=C.payload,typeof w=="function"){m=w.call(k,m,f);break e}m=w;break e;case 3:w.flags=w.flags&-65537|128;case 0:if(w=C.payload,f=typeof w=="function"?w.call(k,m,f):w,f==null)break e;m=Pt({},m,f);break e;case 2:nr=!0}}l.callback!==null&&l.lane!==0&&(e.flags|=64,f=r.effects,f===null?r.effects=[l]:f.push(l))}else k={eventTime:k,lane:f,tag:l.tag,payload:l.payload,callback:l.callback,next:null},d===null?(c=d=k,a=m):d=d.next=k,s|=f;if(l=l.next,l===null){if(l=r.shared.pending,l===null)break;f=l,l=f.next,f.next=null,r.lastBaseUpdate=f,r.shared.pending=null}}while(!0);if(d===null&&(a=m),r.baseState=a,r.firstBaseUpdate=c,r.lastBaseUpdate=d,t=r.shared.interleaved,t!==null){r=t;do s|=r.lane,r=r.next;while(r!==t)}else i===null&&(r.shared.lanes=0);ei|=s,e.lanes=s,e.memoizedState=m}}function Kf(e,t,n){if(e=t.effects,t.effects=null,e!==null)for(t=0;t<e.length;t++){var o=e[t],r=o.callback;if(r!==null){if(o.callback=null,o=n,typeof r!="function")throw Error(Q(191,r));r.call(o)}}}var wl={},To=yr(wl),_l=yr(wl),ml=yr(wl);function Qr(e){if(e===wl)throw Error(Q(174));return e}function O_(e,t){switch(bt(ml,t),bt(_l,e),bt(To,wl),e=t.nodeType,e){case 9:case 11:t=(t=t.documentElement)?t.namespaceURI:Pd(null,"");break;default:e=e===8?t.parentNode:t,t=e.namespaceURI||null,e=e.tagName,t=Pd(t,e)}Ct(To),bt(To,t)}function ji(){Ct(To),Ct(_l),Ct(ml)}function kh(e){Qr(ml.current);var t=Qr(To.current),n=Pd(t,e.type);t!==n&&(bt(_l,e),bt(To,n))}function B_(e){_l.current===e&&(Ct(To),Ct(_l))}var Lt=yr(0);function _c(e){for(var t=e;t!==null;){if(t.tag===13){var n=t.memoizedState;if(n!==null&&(n=n.dehydrated,n===null||n.data==="$?"||n.data==="$!"))return t}else if(t.tag===19&&t.memoizedProps.revealOrder!==void 0){if((t.flags&128)!==0)return t}else if(t.child!==null){t.child.return=t,t=t.child;continue}if(t===e)break;for(;t.sibling===null;){if(t.return===null||t.return===e)return null;t=t.return}t.sibling.return=t.return,t=t.sibling}return null}var pd=[];function z_(){for(var e=0;e<pd.length;e++)pd[e]._workInProgressVersionPrimary=null;pd.length=0}var ja=Vo.ReactCurrentDispatcher,fd=Vo.ReactCurrentBatchConfig,Zr=0,Rt=null,Wt=null,Vt=null,mc=!1,Ks=!1,pl=0,fx=0;function ln(){throw Error(Q(321))}function H_(e,t){if(t===null)return!1;for(var n=0;n<t.length&&n<e.length;n++)if(!mo(e[n],t[n]))return!1;return!0}function W_(e,t,n,o,r,i){if(Zr=i,Rt=t,t.memoizedState=null,t.updateQueue=null,t.lanes=0,ja.current=e===null||e.memoizedState===null?xx:vx,e=n(o,r),Ks){i=0;do{if(Ks=!1,pl=0,25<=i)throw Error(Q(301));i+=1,Vt=Wt=null,t.updateQueue=null,ja.current=wx,e=n(o,r)}while(Ks)}if(ja.current=pc,t=Wt!==null&&Wt.next!==null,Zr=0,Vt=Wt=Rt=null,mc=!1,t)throw Error(Q(300));return e}function U_(){var e=pl!==0;return pl=0,e}function Co(){var e={memoizedState:null,baseState:null,baseQueue:null,queue:null,next:null};return Vt===null?Rt.memoizedState=Vt=e:Vt=Vt.next=e,Vt}function Gn(){if(Wt===null){var e=Rt.alternate;e=e!==null?e.memoizedState:null}else e=Wt.next;var t=Vt===null?Rt.memoizedState:Vt.next;if(t!==null)Vt=t,Wt=e;else{if(e===null)throw Error(Q(310));Wt=e,e={memoizedState:Wt.memoizedState,baseState:Wt.baseState,baseQueue:Wt.baseQueue,queue:Wt.queue,next:null},Vt===null?Rt.memoizedState=Vt=e:Vt=Vt.next=e}return Vt}function fl(e,t){return typeof t=="function"?t(e):t}function gd(e){var t=Gn(),n=t.queue;if(n===null)throw Error(Q(311));n.lastRenderedReducer=e;var o=Wt,r=o.baseQueue,i=n.pending;if(i!==null){if(r!==null){var s=r.next;r.next=i.next,i.next=s}o.baseQueue=r=i,n.pending=null}if(r!==null){i=r.next,o=o.baseState;var l=s=null,a=null,c=i;do{var d=c.lane;if((Zr&d)===d)a!==null&&(a=a.next={lane:0,action:c.action,hasEagerState:c.hasEagerState,eagerState:c.eagerState,next:null}),o=c.hasEagerState?c.eagerState:e(o,c.action);else{var m={lane:d,action:c.action,hasEagerState:c.hasEagerState,eagerState:c.eagerState,next:null};a===null?(l=a=m,s=o):a=a.next=m,Rt.lanes|=d,ei|=d}c=c.next}while(c!==null&&c!==i);a===null?s=o:a.next=l,mo(o,t.memoizedState)||(Sn=!0),t.memoizedState=o,t.baseState=s,t.baseQueue=a,n.lastRenderedState=o}if(e=n.interleaved,e!==null){r=e;do i=r.lane,Rt.lanes|=i,ei|=i,r=r.next;while(r!==e)}else r===null&&(n.lanes=0);return[t.memoizedState,n.dispatch]}function hd(e){var t=Gn(),n=t.queue;if(n===null)throw Error(Q(311));n.lastRenderedReducer=e;var o=n.dispatch,r=n.pending,i=t.memoizedState;if(r!==null){n.pending=null;var s=r=r.next;do i=e(i,s.action),s=s.next;while(s!==r);mo(i,t.memoizedState)||(Sn=!0),t.memoizedState=i,t.baseQueue===null&&(t.baseState=i),n.lastRenderedState=i}return[i,o]}function Sh(){}function Ch(e,t){var n=Rt,o=Gn(),r=t(),i=!mo(o.memoizedState,r);if(i&&(o.memoizedState=r,Sn=!0),o=o.queue,j_(Th.bind(null,n,o,e),[e]),o.getSnapshot!==t||i||Vt!==null&&Vt.memoizedState.tag&1){if(n.flags|=2048,gl(9,Mh.bind(null,n,o,r,t),void 0,null),Xt===null)throw Error(Q(349));(Zr&30)!==0||Eh(n,t,r)}return r}function Eh(e,t,n){e.flags|=16384,e={getSnapshot:t,value:n},t=Rt.updateQueue,t===null?(t={lastEffect:null,stores:null},Rt.updateQueue=t,t.stores=[e]):(n=t.stores,n===null?t.stores=[e]:n.push(e))}function Mh(e,t,n,o){t.value=n,t.getSnapshot=o,Lh(t)&&Rh(e)}function Th(e,t,n){return n(function(){Lh(t)&&Rh(e)})}function Lh(e){var t=e.getSnapshot;e=e.value;try{var n=t();return!mo(e,n)}catch{return!0}}function Rh(e){var t=jo(e,1);t!==null&&_o(t,e,1,-1)}function Jf(e){var t=Co();return typeof e=="function"&&(e=e()),t.memoizedState=t.baseState=e,e={pending:null,interleaved:null,lanes:0,dispatch:null,lastRenderedReducer:fl,lastRenderedState:e},t.queue=e,e=e.dispatch=yx.bind(null,Rt,e),[t.memoizedState,e]}function gl(e,t,n,o){return e={tag:e,create:t,destroy:n,deps:o,next:null},t=Rt.updateQueue,t===null?(t={lastEffect:null,stores:null},Rt.updateQueue=t,t.lastEffect=e.next=e):(n=t.lastEffect,n===null?t.lastEffect=e.next=e:(o=n.next,n.next=e,e.next=o,t.lastEffect=e)),e}function Ph(){return Gn().memoizedState}function Ya(e,t,n,o){var r=Co();Rt.flags|=e,r.memoizedState=gl(1|t,n,void 0,o===void 0?null:o)}function Ec(e,t,n,o){var r=Gn();o=o===void 0?null:o;var i=void 0;if(Wt!==null){var s=Wt.memoizedState;if(i=s.destroy,o!==null&&H_(o,s.deps)){r.memoizedState=gl(t,n,i,o);return}}Rt.flags|=e,r.memoizedState=gl(1|t,n,i,o)}function Zf(e,t){return Ya(8390656,8,e,t)}function j_(e,t){return Ec(2048,8,e,t)}function Nh(e,t){return Ec(4,2,e,t)}function Ih(e,t){return Ec(4,4,e,t)}function Dh(e,t){if(typeof t=="function")return e=e(),t(e),function(){t(null)};if(t!=null)return e=e(),t.current=e,function(){t.current=null}}function $h(e,t,n){return n=n!=null?n.concat([e]):null,Ec(4,4,Dh.bind(null,t,e),n)}function Y_(){}function Ah(e,t){var n=Gn();t=t===void 0?null:t;var o=n.memoizedState;return o!==null&&t!==null&&H_(t,o[1])?o[0]:(n.memoizedState=[e,t],e)}function Fh(e,t){var n=Gn();t=t===void 0?null:t;var o=n.memoizedState;return o!==null&&t!==null&&H_(t,o[1])?o[0]:(e=e(),n.memoizedState=[e,t],e)}function Oh(e,t,n){return(Zr&21)===0?(e.baseState&&(e.baseState=!1,Sn=!0),e.memoizedState=n):(mo(n,t)||(n=Ug(),Rt.lanes|=n,ei|=n,e.baseState=!0),t)}function gx(e,t){var n=mt;mt=n!==0&&4>n?n:4,e(!0);var o=fd.transition;fd.transition={};try{e(!1),t()}finally{mt=n,fd.transition=o}}function Bh(){return Gn().memoizedState}function hx(e,t,n){var o=mr(e);if(n={lane:o,action:n,hasEagerState:!1,eagerState:null,next:null},zh(e))Hh(t,n);else if(n=wh(e,t,n,o),n!==null){var r=gn();_o(n,e,o,r),Wh(n,t,o)}}function yx(e,t,n){var o=mr(e),r={lane:o,action:n,hasEagerState:!1,eagerState:null,next:null};if(zh(e))Hh(t,r);else{var i=e.alternate;if(e.lanes===0&&(i===null||i.lanes===0)&&(i=t.lastRenderedReducer,i!==null))try{var s=t.lastRenderedState,l=i(s,n);if(r.hasEagerState=!0,r.eagerState=l,mo(l,s)){var a=t.interleaved;a===null?(r.next=r,A_(t)):(r.next=a.next,a.next=r),t.interleaved=r;return}}catch{}n=wh(e,t,r,o),n!==null&&(r=gn(),_o(n,e,o,r),Wh(n,t,o))}}function zh(e){var t=e.alternate;return e===Rt||t!==null&&t===Rt}function Hh(e,t){Ks=mc=!0;var n=e.pending;n===null?t.next=t:(t.next=n.next,n.next=t),e.pending=t}function Wh(e,t,n){if((n&4194240)!==0){var o=t.lanes;o&=e.pendingLanes,n|=o,t.lanes=n,b_(e,n)}}var pc={readContext:Qn,useCallback:ln,useContext:ln,useEffect:ln,useImperativeHandle:ln,useInsertionEffect:ln,useLayoutEffect:ln,useMemo:ln,useReducer:ln,useRef:ln,useState:ln,useDebugValue:ln,useDeferredValue:ln,useTransition:ln,useMutableSource:ln,useSyncExternalStore:ln,useId:ln,unstable_isNewReconciler:!1},xx={readContext:Qn,useCallback:function(e,t){return Co().memoizedState=[e,t===void 0?null:t],e},useContext:Qn,useEffect:Zf,useImperativeHandle:function(e,t,n){return n=n!=null?n.concat([e]):null,Ya(4194308,4,Dh.bind(null,t,e),n)},useLayoutEffect:function(e,t){return Ya(4194308,4,e,t)},useInsertionEffect:function(e,t){return Ya(4,2,e,t)},useMemo:function(e,t){var n=Co();return t=t===void 0?null:t,e=e(),n.memoizedState=[e,t],e},useReducer:function(e,t,n){var o=Co();return t=n!==void 0?n(t):t,o.memoizedState=o.baseState=t,e={pending:null,interleaved:null,lanes:0,dispatch:null,lastRenderedReducer:e,lastRenderedState:t},o.queue=e,e=e.dispatch=hx.bind(null,Rt,e),[o.memoizedState,e]},useRef:function(e){var t=Co();return e={current:e},t.memoizedState=e},useState:Jf,useDebugValue:Y_,useDeferredValue:function(e){return Co().memoizedState=e},useTransition:function(){var e=Jf(!1),t=e[0];return e=gx.bind(null,e[1]),Co().memoizedState=e,[t,e]},useMutableSource:function(){},useSyncExternalStore:function(e,t,n){var o=Rt,r=Co();if(Tt){if(n===void 0)throw Error(Q(407));n=n()}else{if(n=t(),Xt===null)throw Error(Q(349));(Zr&30)!==0||Eh(o,t,n)}r.memoizedState=n;var i={value:n,getSnapshot:t};return r.queue=i,Zf(Th.bind(null,o,i,e),[e]),o.flags|=2048,gl(9,Mh.bind(null,o,i,n,t),void 0,null),n},useId:function(){var e=Co(),t=Xt.identifierPrefix;if(Tt){var n=zo,o=Bo;n=(o&~(1<<32-uo(o)-1)).toString(32)+n,t=":"+t+"R"+n,n=pl++,0<n&&(t+="H"+n.toString(32)),t+=":"}else n=fx++,t=":"+t+"r"+n.toString(32)+":";return e.memoizedState=t},unstable_isNewReconciler:!1},vx={readContext:Qn,useCallback:Ah,useContext:Qn,useEffect:j_,useImperativeHandle:$h,useInsertionEffect:Nh,useLayoutEffect:Ih,useMemo:Fh,useReducer:gd,useRef:Ph,useState:function(){return gd(fl)},useDebugValue:Y_,useDeferredValue:function(e){var t=Gn();return Oh(t,Wt.memoizedState,e)},useTransition:function(){var e=gd(fl)[0],t=Gn().memoizedState;return[e,t]},useMutableSource:Sh,useSyncExternalStore:Ch,useId:Bh,unstable_isNewReconciler:!1},wx={readContext:Qn,useCallback:Ah,useContext:Qn,useEffect:j_,useImperativeHandle:$h,useInsertionEffect:Nh,useLayoutEffect:Ih,useMemo:Fh,useReducer:hd,useRef:Ph,useState:function(){return hd(fl)},useDebugValue:Y_,useDeferredValue:function(e){var t=Gn();return Wt===null?t.memoizedState=e:Oh(t,Wt.memoizedState,e)},useTransition:function(){var e=hd(fl)[0],t=Gn().memoizedState;return[e,t]},useMutableSource:Sh,useSyncExternalStore:Ch,useId:Bh,unstable_isNewReconciler:!1};function lo(e,t){if(e&&e.defaultProps){t=Pt({},t),e=e.defaultProps;for(var n in e)t[n]===void 0&&(t[n]=e[n]);return t}return t}function Jd(e,t,n,o){t=e.memoizedState,n=n(o,t),n=n==null?t:Pt({},t,n),e.memoizedState=n,e.lanes===0&&(e.updateQueue.baseState=n)}var Mc={isMounted:function(e){return(e=e._reactInternals)?oi(e)===e:!1},enqueueSetState:function(e,t,n){e=e._reactInternals;var o=gn(),r=mr(e),i=Ho(o,r);i.payload=t,n!=null&&(i.callback=n),t=dr(e,i,r),t!==null&&(_o(t,e,r,o),Ua(t,e,r))},enqueueReplaceState:function(e,t,n){e=e._reactInternals;var o=gn(),r=mr(e),i=Ho(o,r);i.tag=1,i.payload=t,n!=null&&(i.callback=n),t=dr(e,i,r),t!==null&&(_o(t,e,r,o),Ua(t,e,r))},enqueueForceUpdate:function(e,t){e=e._reactInternals;var n=gn(),o=mr(e),r=Ho(n,o);r.tag=2,t!=null&&(r.callback=t),t=dr(e,r,o),t!==null&&(_o(t,e,o,n),Ua(t,e,o))}};function eg(e,t,n,o,r,i,s){return e=e.stateNode,typeof e.shouldComponentUpdate=="function"?e.shouldComponentUpdate(o,i,s):t.prototype&&t.prototype.isPureReactComponent?!al(n,o)||!al(r,i):!0}function Uh(e,t,n){var o=!1,r=gr,i=t.contextType;return typeof i=="object"&&i!==null?i=Qn(i):(r=En(t)?Kr:un.current,o=t.contextTypes,i=(o=o!=null)?Hi(e,r):gr),t=new t(n,i),e.memoizedState=t.state!==null&&t.state!==void 0?t.state:null,t.updater=Mc,e.stateNode=t,t._reactInternals=e,o&&(e=e.stateNode,e.__reactInternalMemoizedUnmaskedChildContext=r,e.__reactInternalMemoizedMaskedChildContext=i),t}function tg(e,t,n,o){e=t.state,typeof t.componentWillReceiveProps=="function"&&t.componentWillReceiveProps(n,o),typeof t.UNSAFE_componentWillReceiveProps=="function"&&t.UNSAFE_componentWillReceiveProps(n,o),t.state!==e&&Mc.enqueueReplaceState(t,t.state,null)}function Zd(e,t,n,o){var r=e.stateNode;r.props=n,r.state=e.memoizedState,r.refs={},F_(e);var i=t.contextType;typeof i=="object"&&i!==null?r.context=Qn(i):(i=En(t)?Kr:un.current,r.context=Hi(e,i)),r.state=e.memoizedState,i=t.getDerivedStateFromProps,typeof i=="function"&&(Jd(e,t,i,n),r.state=e.memoizedState),typeof t.getDerivedStateFromProps=="function"||typeof r.getSnapshotBeforeUpdate=="function"||typeof r.UNSAFE_componentWillMount!="function"&&typeof r.componentWillMount!="function"||(t=r.state,typeof r.componentWillMount=="function"&&r.componentWillMount(),typeof r.UNSAFE_componentWillMount=="function"&&r.UNSAFE_componentWillMount(),t!==r.state&&Mc.enqueueReplaceState(r,r.state,null),dc(e,n,r,o),r.state=e.memoizedState),typeof r.componentDidMount=="function"&&(e.flags|=4194308)}function Yi(e,t){try{var n="",o=t;do n+=K5(o),o=o.return;while(o);var r=n}catch(i){r=`
Error generating stack: `+i.message+`
`+i.stack}return{value:e,source:t,stack:r,digest:null}}function yd(e,t,n){return{value:e,source:null,stack:n??null,digest:t??null}}function e_(e,t){try{console.error(t.value)}catch(n){setTimeout(function(){throw n})}}var bx=typeof WeakMap=="function"?WeakMap:Map;function jh(e,t,n){n=Ho(-1,n),n.tag=3,n.payload={element:null};var o=t.value;return n.callback=function(){gc||(gc=!0,u_=o),e_(e,t)},n}function Yh(e,t,n){n=Ho(-1,n),n.tag=3;var o=e.type.getDerivedStateFromError;if(typeof o=="function"){var r=t.value;n.payload=function(){return o(r)},n.callback=function(){e_(e,t)}}var i=e.stateNode;return i!==null&&typeof i.componentDidCatch=="function"&&(n.callback=function(){e_(e,t),typeof o!="function"&&(_r===null?_r=new Set([this]):_r.add(this));var s=t.stack;this.componentDidCatch(t.value,{componentStack:s!==null?s:""})}),n}function ng(e,t,n){var o=e.pingCache;if(o===null){o=e.pingCache=new bx;var r=new Set;o.set(t,r)}else r=o.get(t),r===void 0&&(r=new Set,o.set(t,r));r.has(n)||(r.add(n),e=Ax.bind(null,e,t,n),t.then(e,e))}function og(e){do{var t;if((t=e.tag===13)&&(t=e.memoizedState,t=t!==null?t.dehydrated!==null:!0),t)return e;e=e.return}while(e!==null);return null}function rg(e,t,n,o,r){return(e.mode&1)===0?(e===t?e.flags|=65536:(e.flags|=128,n.flags|=131072,n.flags&=-52805,n.tag===1&&(n.alternate===null?n.tag=17:(t=Ho(-1,1),t.tag=2,dr(n,t,1))),n.lanes|=1),e):(e.flags|=65536,e.lanes=r,e)}var kx=Vo.ReactCurrentOwner,Sn=!1;function fn(e,t,n,o){t.child=e===null?vh(t,null,n,o):Ui(t,e.child,n,o)}function ig(e,t,n,o,r){n=n.render;var i=t.ref;return Oi(t,r),o=W_(e,t,n,o,i,r),n=U_(),e!==null&&!Sn?(t.updateQueue=e.updateQueue,t.flags&=-2053,e.lanes&=~r,Yo(e,t,r)):(Tt&&n&&R_(t),t.flags|=1,fn(e,t,o,r),t.child)}function sg(e,t,n,o,r){if(e===null){var i=n.type;return typeof i=="function"&&!Z_(i)&&i.defaultProps===void 0&&n.compare===null&&n.defaultProps===void 0?(t.tag=15,t.type=i,Vh(e,t,i,o,r)):(e=Ga(n.type,null,o,t,t.mode,r),e.ref=t.ref,e.return=t,t.child=e)}if(i=e.child,(e.lanes&r)===0){var s=i.memoizedProps;if(n=n.compare,n=n!==null?n:al,n(s,o)&&e.ref===t.ref)return Yo(e,t,r)}return t.flags|=1,e=pr(i,o),e.ref=t.ref,e.return=t,t.child=e}function Vh(e,t,n,o,r){if(e!==null){var i=e.memoizedProps;if(al(i,o)&&e.ref===t.ref)if(Sn=!1,t.pendingProps=o=i,(e.lanes&r)!==0)(e.flags&131072)!==0&&(Sn=!0);else return t.lanes=e.lanes,Yo(e,t,r)}return t_(e,t,n,o,r)}function Xh(e,t,n){var o=t.pendingProps,r=o.children,i=e!==null?e.memoizedState:null;if(o.mode==="hidden")if((t.mode&1)===0)t.memoizedState={baseLanes:0,cachePool:null,transitions:null},bt(Ii,Pn),Pn|=n;else{if((n&1073741824)===0)return e=i!==null?i.baseLanes|n:n,t.lanes=t.childLanes=1073741824,t.memoizedState={baseLanes:e,cachePool:null,transitions:null},t.updateQueue=null,bt(Ii,Pn),Pn|=e,null;t.memoizedState={baseLanes:0,cachePool:null,transitions:null},o=i!==null?i.baseLanes:n,bt(Ii,Pn),Pn|=o}else i!==null?(o=i.baseLanes|n,t.memoizedState=null):o=n,bt(Ii,Pn),Pn|=o;return fn(e,t,r,n),t.child}function Qh(e,t){var n=t.ref;(e===null&&n!==null||e!==null&&e.ref!==n)&&(t.flags|=512,t.flags|=2097152)}function t_(e,t,n,o,r){var i=En(n)?Kr:un.current;return i=Hi(t,i),Oi(t,r),n=W_(e,t,n,o,i,r),o=U_(),e!==null&&!Sn?(t.updateQueue=e.updateQueue,t.flags&=-2053,e.lanes&=~r,Yo(e,t,r)):(Tt&&o&&R_(t),t.flags|=1,fn(e,t,n,r),t.child)}function lg(e,t,n,o,r){if(En(n)){var i=!0;sc(t)}else i=!1;if(Oi(t,r),t.stateNode===null)Va(e,t),Uh(t,n,o),Zd(t,n,o,r),o=!0;else if(e===null){var s=t.stateNode,l=t.memoizedProps;s.props=l;var a=s.context,c=n.contextType;typeof c=="object"&&c!==null?c=Qn(c):(c=En(n)?Kr:un.current,c=Hi(t,c));var d=n.getDerivedStateFromProps,m=typeof d=="function"||typeof s.getSnapshotBeforeUpdate=="function";m||typeof s.UNSAFE_componentWillReceiveProps!="function"&&typeof s.componentWillReceiveProps!="function"||(l!==o||a!==c)&&tg(t,s,o,c),nr=!1;var f=t.memoizedState;s.state=f,dc(t,o,s,r),a=t.memoizedState,l!==o||f!==a||Cn.current||nr?(typeof d=="function"&&(Jd(t,n,d,o),a=t.memoizedState),(l=nr||eg(t,n,l,o,f,a,c))?(m||typeof s.UNSAFE_componentWillMount!="function"&&typeof s.componentWillMount!="function"||(typeof s.componentWillMount=="function"&&s.componentWillMount(),typeof s.UNSAFE_componentWillMount=="function"&&s.UNSAFE_componentWillMount()),typeof s.componentDidMount=="function"&&(t.flags|=4194308)):(typeof s.componentDidMount=="function"&&(t.flags|=4194308),t.memoizedProps=o,t.memoizedState=a),s.props=o,s.state=a,s.context=c,o=l):(typeof s.componentDidMount=="function"&&(t.flags|=4194308),o=!1)}else{s=t.stateNode,bh(e,t),l=t.memoizedProps,c=t.type===t.elementType?l:lo(t.type,l),s.props=c,m=t.pendingProps,f=s.context,a=n.contextType,typeof a=="object"&&a!==null?a=Qn(a):(a=En(n)?Kr:un.current,a=Hi(t,a));var k=n.getDerivedStateFromProps;(d=typeof k=="function"||typeof s.getSnapshotBeforeUpdate=="function")||typeof s.UNSAFE_componentWillReceiveProps!="function"&&typeof s.componentWillReceiveProps!="function"||(l!==m||f!==a)&&tg(t,s,o,a),nr=!1,f=t.memoizedState,s.state=f,dc(t,o,s,r);var w=t.memoizedState;l!==m||f!==w||Cn.current||nr?(typeof k=="function"&&(Jd(t,n,k,o),w=t.memoizedState),(c=nr||eg(t,n,c,o,f,w,a)||!1)?(d||typeof s.UNSAFE_componentWillUpdate!="function"&&typeof s.componentWillUpdate!="function"||(typeof s.componentWillUpdate=="function"&&s.componentWillUpdate(o,w,a),typeof s.UNSAFE_componentWillUpdate=="function"&&s.UNSAFE_componentWillUpdate(o,w,a)),typeof s.componentDidUpdate=="function"&&(t.flags|=4),typeof s.getSnapshotBeforeUpdate=="function"&&(t.flags|=1024)):(typeof s.componentDidUpdate!="function"||l===e.memoizedProps&&f===e.memoizedState||(t.flags|=4),typeof s.getSnapshotBeforeUpdate!="function"||l===e.memoizedProps&&f===e.memoizedState||(t.flags|=1024),t.memoizedProps=o,t.memoizedState=w),s.props=o,s.state=w,s.context=a,o=c):(typeof s.componentDidUpdate!="function"||l===e.memoizedProps&&f===e.memoizedState||(t.flags|=4),typeof s.getSnapshotBeforeUpdate!="function"||l===e.memoizedProps&&f===e.memoizedState||(t.flags|=1024),o=!1)}return n_(e,t,n,o,i,r)}function n_(e,t,n,o,r,i){Qh(e,t);var s=(t.flags&128)!==0;if(!o&&!s)return r&&Vf(t,n,!1),Yo(e,t,i);o=t.stateNode,kx.current=t;var l=s&&typeof n.getDerivedStateFromError!="function"?null:o.render();return t.flags|=1,e!==null&&s?(t.child=Ui(t,e.child,null,i),t.child=Ui(t,null,l,i)):fn(e,t,l,i),t.memoizedState=o.state,r&&Vf(t,n,!0),t.child}function Gh(e){var t=e.stateNode;t.pendingContext?Yf(e,t.pendingContext,t.pendingContext!==t.context):t.context&&Yf(e,t.context,!1),O_(e,t.containerInfo)}function ag(e,t,n,o,r){return Wi(),N_(r),t.flags|=256,fn(e,t,n,o),t.child}var o_={dehydrated:null,treeContext:null,retryLane:0};function r_(e){return{baseLanes:e,cachePool:null,transitions:null}}function qh(e,t,n){var o=t.pendingProps,r=Lt.current,i=!1,s=(t.flags&128)!==0,l;if((l=s)||(l=e!==null&&e.memoizedState===null?!1:(r&2)!==0),l?(i=!0,t.flags&=-129):(e===null||e.memoizedState!==null)&&(r|=1),bt(Lt,r&1),e===null)return qd(t),e=t.memoizedState,e!==null&&(e=e.dehydrated,e!==null)?((t.mode&1)===0?t.lanes=1:e.data==="$!"?t.lanes=8:t.lanes=1073741824,null):(s=o.children,e=o.fallback,i?(o=t.mode,i=t.child,s={mode:"hidden",children:s},(o&1)===0&&i!==null?(i.childLanes=0,i.pendingProps=s):i=Rc(s,o,0,null),e=qr(e,o,n,null),i.return=t,e.return=t,i.sibling=e,t.child=i,t.child.memoizedState=r_(n),t.memoizedState=o_,e):V_(t,s));if(r=e.memoizedState,r!==null&&(l=r.dehydrated,l!==null))return Sx(e,t,s,o,l,r,n);if(i){i=o.fallback,s=t.mode,r=e.child,l=r.sibling;var a={mode:"hidden",children:o.children};return(s&1)===0&&t.child!==r?(o=t.child,o.childLanes=0,o.pendingProps=a,t.deletions=null):(o=pr(r,a),o.subtreeFlags=r.subtreeFlags&14680064),l!==null?i=pr(l,i):(i=qr(i,s,n,null),i.flags|=2),i.return=t,o.return=t,o.sibling=i,t.child=o,o=i,i=t.child,s=e.child.memoizedState,s=s===null?r_(n):{baseLanes:s.baseLanes|n,cachePool:null,transitions:s.transitions},i.memoizedState=s,i.childLanes=e.childLanes&~n,t.memoizedState=o_,o}return i=e.child,e=i.sibling,o=pr(i,{mode:"visible",children:o.children}),(t.mode&1)===0&&(o.lanes=n),o.return=t,o.sibling=null,e!==null&&(n=t.deletions,n===null?(t.deletions=[e],t.flags|=16):n.push(e)),t.child=o,t.memoizedState=null,o}function V_(e,t){return t=Rc({mode:"visible",children:t},e.mode,0,null),t.return=e,e.child=t}function Fa(e,t,n,o){return o!==null&&N_(o),Ui(t,e.child,null,n),e=V_(t,t.pendingProps.children),e.flags|=2,t.memoizedState=null,e}function Sx(e,t,n,o,r,i,s){if(n)return t.flags&256?(t.flags&=-257,o=yd(Error(Q(422))),Fa(e,t,s,o)):t.memoizedState!==null?(t.child=e.child,t.flags|=128,null):(i=o.fallback,r=t.mode,o=Rc({mode:"visible",children:o.children},r,0,null),i=qr(i,r,s,null),i.flags|=2,o.return=t,i.return=t,o.sibling=i,t.child=o,(t.mode&1)!==0&&Ui(t,e.child,null,s),t.child.memoizedState=r_(s),t.memoizedState=o_,i);if((t.mode&1)===0)return Fa(e,t,s,null);if(r.data==="$!"){if(o=r.nextSibling&&r.nextSibling.dataset,o)var l=o.dgst;return o=l,i=Error(Q(419)),o=yd(i,o,void 0),Fa(e,t,s,o)}if(l=(s&e.childLanes)!==0,Sn||l){if(o=Xt,o!==null){switch(s&-s){case 4:r=2;break;case 16:r=8;break;case 64:case 128:case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:case 262144:case 524288:case 1048576:case 2097152:case 4194304:case 8388608:case 16777216:case 33554432:case 67108864:r=32;break;case 536870912:r=268435456;break;default:r=0}r=(r&(o.suspendedLanes|s))!==0?0:r,r!==0&&r!==i.retryLane&&(i.retryLane=r,jo(e,r),_o(o,e,r,-1))}return J_(),o=yd(Error(Q(421))),Fa(e,t,s,o)}return r.data==="$?"?(t.flags|=128,t.child=e.child,t=Fx.bind(null,e),r._reactRetry=t,null):(e=i.treeContext,Nn=ur(r.nextSibling),In=t,Tt=!0,co=null,e!==null&&(jn[Yn++]=Bo,jn[Yn++]=zo,jn[Yn++]=Jr,Bo=e.id,zo=e.overflow,Jr=t),t=V_(t,o.children),t.flags|=4096,t)}function cg(e,t,n){e.lanes|=t;var o=e.alternate;o!==null&&(o.lanes|=t),Kd(e.return,t,n)}function xd(e,t,n,o,r){var i=e.memoizedState;i===null?e.memoizedState={isBackwards:t,rendering:null,renderingStartTime:0,last:o,tail:n,tailMode:r}:(i.isBackwards=t,i.rendering=null,i.renderingStartTime=0,i.last=o,i.tail=n,i.tailMode=r)}function Kh(e,t,n){var o=t.pendingProps,r=o.revealOrder,i=o.tail;if(fn(e,t,o.children,n),o=Lt.current,(o&2)!==0)o=o&1|2,t.flags|=128;else{if(e!==null&&(e.flags&128)!==0)e:for(e=t.child;e!==null;){if(e.tag===13)e.memoizedState!==null&&cg(e,n,t);else if(e.tag===19)cg(e,n,t);else if(e.child!==null){e.child.return=e,e=e.child;continue}if(e===t)break e;for(;e.sibling===null;){if(e.return===null||e.return===t)break e;e=e.return}e.sibling.return=e.return,e=e.sibling}o&=1}if(bt(Lt,o),(t.mode&1)===0)t.memoizedState=null;else switch(r){case"forwards":for(n=t.child,r=null;n!==null;)e=n.alternate,e!==null&&_c(e)===null&&(r=n),n=n.sibling;n=r,n===null?(r=t.child,t.child=null):(r=n.sibling,n.sibling=null),xd(t,!1,r,n,i);break;case"backwards":for(n=null,r=t.child,t.child=null;r!==null;){if(e=r.alternate,e!==null&&_c(e)===null){t.child=r;break}e=r.sibling,r.sibling=n,n=r,r=e}xd(t,!0,n,null,i);break;case"together":xd(t,!1,null,null,void 0);break;default:t.memoizedState=null}return t.child}function Va(e,t){(t.mode&1)===0&&e!==null&&(e.alternate=null,t.alternate=null,t.flags|=2)}function Yo(e,t,n){if(e!==null&&(t.dependencies=e.dependencies),ei|=t.lanes,(n&t.childLanes)===0)return null;if(e!==null&&t.child!==e.child)throw Error(Q(153));if(t.child!==null){for(e=t.child,n=pr(e,e.pendingProps),t.child=n,n.return=t;e.sibling!==null;)e=e.sibling,n=n.sibling=pr(e,e.pendingProps),n.return=t;n.sibling=null}return t.child}function Cx(e,t,n){switch(t.tag){case 3:Gh(t),Wi();break;case 5:kh(t);break;case 1:En(t.type)&&sc(t);break;case 4:O_(t,t.stateNode.containerInfo);break;case 10:var o=t.type._context,r=t.memoizedProps.value;bt(cc,o._currentValue),o._currentValue=r;break;case 13:if(o=t.memoizedState,o!==null)return o.dehydrated!==null?(bt(Lt,Lt.current&1),t.flags|=128,null):(n&t.child.childLanes)!==0?qh(e,t,n):(bt(Lt,Lt.current&1),e=Yo(e,t,n),e!==null?e.sibling:null);bt(Lt,Lt.current&1);break;case 19:if(o=(n&t.childLanes)!==0,(e.flags&128)!==0){if(o)return Kh(e,t,n);t.flags|=128}if(r=t.memoizedState,r!==null&&(r.rendering=null,r.tail=null,r.lastEffect=null),bt(Lt,Lt.current),o)break;return null;case 22:case 23:return t.lanes=0,Xh(e,t,n)}return Yo(e,t,n)}var Jh,i_,Zh,e0;Jh=function(e,t){for(var n=t.child;n!==null;){if(n.tag===5||n.tag===6)e.appendChild(n.stateNode);else if(n.tag!==4&&n.child!==null){n.child.return=n,n=n.child;continue}if(n===t)break;for(;n.sibling===null;){if(n.return===null||n.return===t)return;n=n.return}n.sibling.return=n.return,n=n.sibling}};i_=function(){};Zh=function(e,t,n,o){var r=e.memoizedProps;if(r!==o){e=t.stateNode,Qr(To.current);var i=null;switch(n){case"input":r=Md(e,r),o=Md(e,o),i=[];break;case"select":r=Pt({},r,{value:void 0}),o=Pt({},o,{value:void 0}),i=[];break;case"textarea":r=Rd(e,r),o=Rd(e,o),i=[];break;default:typeof r.onClick!="function"&&typeof o.onClick=="function"&&(e.onclick=rc)}Nd(n,o);var s;n=null;for(c in r)if(!o.hasOwnProperty(c)&&r.hasOwnProperty(c)&&r[c]!=null)if(c==="style"){var l=r[c];for(s in l)l.hasOwnProperty(s)&&(n||(n={}),n[s]="")}else c!=="dangerouslySetInnerHTML"&&c!=="children"&&c!=="suppressContentEditableWarning"&&c!=="suppressHydrationWarning"&&c!=="autoFocus"&&(tl.hasOwnProperty(c)?i||(i=[]):(i=i||[]).push(c,null));for(c in o){var a=o[c];if(l=r?.[c],o.hasOwnProperty(c)&&a!==l&&(a!=null||l!=null))if(c==="style")if(l){for(s in l)!l.hasOwnProperty(s)||a&&a.hasOwnProperty(s)||(n||(n={}),n[s]="");for(s in a)a.hasOwnProperty(s)&&l[s]!==a[s]&&(n||(n={}),n[s]=a[s])}else n||(i||(i=[]),i.push(c,n)),n=a;else c==="dangerouslySetInnerHTML"?(a=a?a.__html:void 0,l=l?l.__html:void 0,a!=null&&l!==a&&(i=i||[]).push(c,a)):c==="children"?typeof a!="string"&&typeof a!="number"||(i=i||[]).push(c,""+a):c!=="suppressContentEditableWarning"&&c!=="suppressHydrationWarning"&&(tl.hasOwnProperty(c)?(a!=null&&c==="onScroll"&&St("scroll",e),i||l===a||(i=[])):(i=i||[]).push(c,a))}n&&(i=i||[]).push("style",n);var c=i;(t.updateQueue=c)&&(t.flags|=4)}};e0=function(e,t,n,o){n!==o&&(t.flags|=4)};function Bs(e,t){if(!Tt)switch(e.tailMode){case"hidden":t=e.tail;for(var n=null;t!==null;)t.alternate!==null&&(n=t),t=t.sibling;n===null?e.tail=null:n.sibling=null;break;case"collapsed":n=e.tail;for(var o=null;n!==null;)n.alternate!==null&&(o=n),n=n.sibling;o===null?t||e.tail===null?e.tail=null:e.tail.sibling=null:o.sibling=null}}function an(e){var t=e.alternate!==null&&e.alternate.child===e.child,n=0,o=0;if(t)for(var r=e.child;r!==null;)n|=r.lanes|r.childLanes,o|=r.subtreeFlags&14680064,o|=r.flags&14680064,r.return=e,r=r.sibling;else for(r=e.child;r!==null;)n|=r.lanes|r.childLanes,o|=r.subtreeFlags,o|=r.flags,r.return=e,r=r.sibling;return e.subtreeFlags|=o,e.childLanes=n,t}function Ex(e,t,n){var o=t.pendingProps;switch(P_(t),t.tag){case 2:case 16:case 15:case 0:case 11:case 7:case 8:case 12:case 9:case 14:return an(t),null;case 1:return En(t.type)&&ic(),an(t),null;case 3:return o=t.stateNode,ji(),Ct(Cn),Ct(un),z_(),o.pendingContext&&(o.context=o.pendingContext,o.pendingContext=null),(e===null||e.child===null)&&($a(t)?t.flags|=4:e===null||e.memoizedState.isDehydrated&&(t.flags&256)===0||(t.flags|=1024,co!==null&&(m_(co),co=null))),i_(e,t),an(t),null;case 5:B_(t);var r=Qr(ml.current);if(n=t.type,e!==null&&t.stateNode!=null)Zh(e,t,n,o,r),e.ref!==t.ref&&(t.flags|=512,t.flags|=2097152);else{if(!o){if(t.stateNode===null)throw Error(Q(166));return an(t),null}if(e=Qr(To.current),$a(t)){o=t.stateNode,n=t.type;var i=t.memoizedProps;switch(o[Eo]=t,o[dl]=i,e=(t.mode&1)!==0,n){case"dialog":St("cancel",o),St("close",o);break;case"iframe":case"object":case"embed":St("load",o);break;case"video":case"audio":for(r=0;r<Ys.length;r++)St(Ys[r],o);break;case"source":St("error",o);break;case"img":case"image":case"link":St("error",o),St("load",o);break;case"details":St("toggle",o);break;case"input":hf(o,i),St("invalid",o);break;case"select":o._wrapperState={wasMultiple:!!i.multiple},St("invalid",o);break;case"textarea":xf(o,i),St("invalid",o)}Nd(n,i),r=null;for(var s in i)if(i.hasOwnProperty(s)){var l=i[s];s==="children"?typeof l=="string"?o.textContent!==l&&(i.suppressHydrationWarning!==!0&&Da(o.textContent,l,e),r=["children",l]):typeof l=="number"&&o.textContent!==""+l&&(i.suppressHydrationWarning!==!0&&Da(o.textContent,l,e),r=["children",""+l]):tl.hasOwnProperty(s)&&l!=null&&s==="onScroll"&&St("scroll",o)}switch(n){case"input":ba(o),yf(o,i,!0);break;case"textarea":ba(o),vf(o);break;case"select":case"option":break;default:typeof i.onClick=="function"&&(o.onclick=rc)}o=r,t.updateQueue=o,o!==null&&(t.flags|=4)}else{s=r.nodeType===9?r:r.ownerDocument,e==="http://www.w3.org/1999/xhtml"&&(e=Tg(n)),e==="http://www.w3.org/1999/xhtml"?n==="script"?(e=s.createElement("div"),e.innerHTML="<script><\/script>",e=e.removeChild(e.firstChild)):typeof o.is=="string"?e=s.createElement(n,{is:o.is}):(e=s.createElement(n),n==="select"&&(s=e,o.multiple?s.multiple=!0:o.size&&(s.size=o.size))):e=s.createElementNS(e,n),e[Eo]=t,e[dl]=o,Jh(e,t,!1,!1),t.stateNode=e;e:{switch(s=Id(n,o),n){case"dialog":St("cancel",e),St("close",e),r=o;break;case"iframe":case"object":case"embed":St("load",e),r=o;break;case"video":case"audio":for(r=0;r<Ys.length;r++)St(Ys[r],e);r=o;break;case"source":St("error",e),r=o;break;case"img":case"image":case"link":St("error",e),St("load",e),r=o;break;case"details":St("toggle",e),r=o;break;case"input":hf(e,o),r=Md(e,o),St("invalid",e);break;case"option":r=o;break;case"select":e._wrapperState={wasMultiple:!!o.multiple},r=Pt({},o,{value:void 0}),St("invalid",e);break;case"textarea":xf(e,o),r=Rd(e,o),St("invalid",e);break;default:r=o}Nd(n,r),l=r;for(i in l)if(l.hasOwnProperty(i)){var a=l[i];i==="style"?Pg(e,a):i==="dangerouslySetInnerHTML"?(a=a?a.__html:void 0,a!=null&&Lg(e,a)):i==="children"?typeof a=="string"?(n!=="textarea"||a!=="")&&nl(e,a):typeof a=="number"&&nl(e,""+a):i!=="suppressContentEditableWarning"&&i!=="suppressHydrationWarning"&&i!=="autoFocus"&&(tl.hasOwnProperty(i)?a!=null&&i==="onScroll"&&St("scroll",e):a!=null&&g_(e,i,a,s))}switch(n){case"input":ba(e),yf(e,o,!1);break;case"textarea":ba(e),vf(e);break;case"option":o.value!=null&&e.setAttribute("value",""+fr(o.value));break;case"select":e.multiple=!!o.multiple,i=o.value,i!=null?Di(e,!!o.multiple,i,!1):o.defaultValue!=null&&Di(e,!!o.multiple,o.defaultValue,!0);break;default:typeof r.onClick=="function"&&(e.onclick=rc)}switch(n){case"button":case"input":case"select":case"textarea":o=!!o.autoFocus;break e;case"img":o=!0;break e;default:o=!1}}o&&(t.flags|=4)}t.ref!==null&&(t.flags|=512,t.flags|=2097152)}return an(t),null;case 6:if(e&&t.stateNode!=null)e0(e,t,e.memoizedProps,o);else{if(typeof o!="string"&&t.stateNode===null)throw Error(Q(166));if(n=Qr(ml.current),Qr(To.current),$a(t)){if(o=t.stateNode,n=t.memoizedProps,o[Eo]=t,(i=o.nodeValue!==n)&&(e=In,e!==null))switch(e.tag){case 3:Da(o.nodeValue,n,(e.mode&1)!==0);break;case 5:e.memoizedProps.suppressHydrationWarning!==!0&&Da(o.nodeValue,n,(e.mode&1)!==0)}i&&(t.flags|=4)}else o=(n.nodeType===9?n:n.ownerDocument).createTextNode(o),o[Eo]=t,t.stateNode=o}return an(t),null;case 13:if(Ct(Lt),o=t.memoizedState,e===null||e.memoizedState!==null&&e.memoizedState.dehydrated!==null){if(Tt&&Nn!==null&&(t.mode&1)!==0&&(t.flags&128)===0)yh(),Wi(),t.flags|=98560,i=!1;else if(i=$a(t),o!==null&&o.dehydrated!==null){if(e===null){if(!i)throw Error(Q(318));if(i=t.memoizedState,i=i!==null?i.dehydrated:null,!i)throw Error(Q(317));i[Eo]=t}else Wi(),(t.flags&128)===0&&(t.memoizedState=null),t.flags|=4;an(t),i=!1}else co!==null&&(m_(co),co=null),i=!0;if(!i)return t.flags&65536?t:null}return(t.flags&128)!==0?(t.lanes=n,t):(o=o!==null,o!==(e!==null&&e.memoizedState!==null)&&o&&(t.child.flags|=8192,(t.mode&1)!==0&&(e===null||(Lt.current&1)!==0?Ut===0&&(Ut=3):J_())),t.updateQueue!==null&&(t.flags|=4),an(t),null);case 4:return ji(),i_(e,t),e===null&&cl(t.stateNode.containerInfo),an(t),null;case 10:return $_(t.type._context),an(t),null;case 17:return En(t.type)&&ic(),an(t),null;case 19:if(Ct(Lt),i=t.memoizedState,i===null)return an(t),null;if(o=(t.flags&128)!==0,s=i.rendering,s===null)if(o)Bs(i,!1);else{if(Ut!==0||e!==null&&(e.flags&128)!==0)for(e=t.child;e!==null;){if(s=_c(e),s!==null){for(t.flags|=128,Bs(i,!1),o=s.updateQueue,o!==null&&(t.updateQueue=o,t.flags|=4),t.subtreeFlags=0,o=n,n=t.child;n!==null;)i=n,e=o,i.flags&=14680066,s=i.alternate,s===null?(i.childLanes=0,i.lanes=e,i.child=null,i.subtreeFlags=0,i.memoizedProps=null,i.memoizedState=null,i.updateQueue=null,i.dependencies=null,i.stateNode=null):(i.childLanes=s.childLanes,i.lanes=s.lanes,i.child=s.child,i.subtreeFlags=0,i.deletions=null,i.memoizedProps=s.memoizedProps,i.memoizedState=s.memoizedState,i.updateQueue=s.updateQueue,i.type=s.type,e=s.dependencies,i.dependencies=e===null?null:{lanes:e.lanes,firstContext:e.firstContext}),n=n.sibling;return bt(Lt,Lt.current&1|2),t.child}e=e.sibling}i.tail!==null&&At()>Vi&&(t.flags|=128,o=!0,Bs(i,!1),t.lanes=4194304)}else{if(!o)if(e=_c(s),e!==null){if(t.flags|=128,o=!0,n=e.updateQueue,n!==null&&(t.updateQueue=n,t.flags|=4),Bs(i,!0),i.tail===null&&i.tailMode==="hidden"&&!s.alternate&&!Tt)return an(t),null}else 2*At()-i.renderingStartTime>Vi&&n!==1073741824&&(t.flags|=128,o=!0,Bs(i,!1),t.lanes=4194304);i.isBackwards?(s.sibling=t.child,t.child=s):(n=i.last,n!==null?n.sibling=s:t.child=s,i.last=s)}return i.tail!==null?(t=i.tail,i.rendering=t,i.tail=t.sibling,i.renderingStartTime=At(),t.sibling=null,n=Lt.current,bt(Lt,o?n&1|2:n&1),t):(an(t),null);case 22:case 23:return K_(),o=t.memoizedState!==null,e!==null&&e.memoizedState!==null!==o&&(t.flags|=8192),o&&(t.mode&1)!==0?(Pn&1073741824)!==0&&(an(t),t.subtreeFlags&6&&(t.flags|=8192)):an(t),null;case 24:return null;case 25:return null}throw Error(Q(156,t.tag))}function Mx(e,t){switch(P_(t),t.tag){case 1:return En(t.type)&&ic(),e=t.flags,e&65536?(t.flags=e&-65537|128,t):null;case 3:return ji(),Ct(Cn),Ct(un),z_(),e=t.flags,(e&65536)!==0&&(e&128)===0?(t.flags=e&-65537|128,t):null;case 5:return B_(t),null;case 13:if(Ct(Lt),e=t.memoizedState,e!==null&&e.dehydrated!==null){if(t.alternate===null)throw Error(Q(340));Wi()}return e=t.flags,e&65536?(t.flags=e&-65537|128,t):null;case 19:return Ct(Lt),null;case 4:return ji(),null;case 10:return $_(t.type._context),null;case 22:case 23:return K_(),null;case 24:return null;default:return null}}var Oa=!1,cn=!1,Tx=typeof WeakSet=="function"?WeakSet:Set,se=null;function Ni(e,t){var n=e.ref;if(n!==null)if(typeof n=="function")try{n(null)}catch(o){Dt(e,t,o)}else n.current=null}function s_(e,t,n){try{n()}catch(o){Dt(e,t,o)}}var ug=!1;function Lx(e,t){if(Ud=tc,e=ih(),L_(e)){if("selectionStart"in e)var n={start:e.selectionStart,end:e.selectionEnd};else e:{n=(n=e.ownerDocument)&&n.defaultView||window;var o=n.getSelection&&n.getSelection();if(o&&o.rangeCount!==0){n=o.anchorNode;var r=o.anchorOffset,i=o.focusNode;o=o.focusOffset;try{n.nodeType,i.nodeType}catch{n=null;break e}var s=0,l=-1,a=-1,c=0,d=0,m=e,f=null;t:for(;;){for(var k;m!==n||r!==0&&m.nodeType!==3||(l=s+r),m!==i||o!==0&&m.nodeType!==3||(a=s+o),m.nodeType===3&&(s+=m.nodeValue.length),(k=m.firstChild)!==null;)f=m,m=k;for(;;){if(m===e)break t;if(f===n&&++c===r&&(l=s),f===i&&++d===o&&(a=s),(k=m.nextSibling)!==null)break;m=f,f=m.parentNode}m=k}n=l===-1||a===-1?null:{start:l,end:a}}else n=null}n=n||{start:0,end:0}}else n=null;for(jd={focusedElem:e,selectionRange:n},tc=!1,se=t;se!==null;)if(t=se,e=t.child,(t.subtreeFlags&1028)!==0&&e!==null)e.return=t,se=e;else for(;se!==null;){t=se;try{var w=t.alternate;if((t.flags&1024)!==0)switch(t.tag){case 0:case 11:case 15:break;case 1:if(w!==null){var C=w.memoizedProps,S=w.memoizedState,g=t.stateNode,x=g.getSnapshotBeforeUpdate(t.elementType===t.type?C:lo(t.type,C),S);g.__reactInternalSnapshotBeforeUpdate=x}break;case 3:var h=t.stateNode.containerInfo;h.nodeType===1?h.textContent="":h.nodeType===9&&h.documentElement&&h.removeChild(h.documentElement);break;case 5:case 6:case 4:case 17:break;default:throw Error(Q(163))}}catch(E){Dt(t,t.return,E)}if(e=t.sibling,e!==null){e.return=t.return,se=e;break}se=t.return}return w=ug,ug=!1,w}function Js(e,t,n){var o=t.updateQueue;if(o=o!==null?o.lastEffect:null,o!==null){var r=o=o.next;do{if((r.tag&e)===e){var i=r.destroy;r.destroy=void 0,i!==void 0&&s_(t,n,i)}r=r.next}while(r!==o)}}function Tc(e,t){if(t=t.updateQueue,t=t!==null?t.lastEffect:null,t!==null){var n=t=t.next;do{if((n.tag&e)===e){var o=n.create;n.destroy=o()}n=n.next}while(n!==t)}}function l_(e){var t=e.ref;if(t!==null){var n=e.stateNode;e.tag,e=n,typeof t=="function"?t(e):t.current=e}}function t0(e){var t=e.alternate;t!==null&&(e.alternate=null,t0(t)),e.child=null,e.deletions=null,e.sibling=null,e.tag===5&&(t=e.stateNode,t!==null&&(delete t[Eo],delete t[dl],delete t[Xd],delete t[dx],delete t[_x])),e.stateNode=null,e.return=null,e.dependencies=null,e.memoizedProps=null,e.memoizedState=null,e.pendingProps=null,e.stateNode=null,e.updateQueue=null}function n0(e){return e.tag===5||e.tag===3||e.tag===4}function dg(e){e:for(;;){for(;e.sibling===null;){if(e.return===null||n0(e.return))return null;e=e.return}for(e.sibling.return=e.return,e=e.sibling;e.tag!==5&&e.tag!==6&&e.tag!==18;){if(e.flags&2||e.child===null||e.tag===4)continue e;e.child.return=e,e=e.child}if(!(e.flags&2))return e.stateNode}}function a_(e,t,n){var o=e.tag;if(o===5||o===6)e=e.stateNode,t?n.nodeType===8?n.parentNode.insertBefore(e,t):n.insertBefore(e,t):(n.nodeType===8?(t=n.parentNode,t.insertBefore(e,n)):(t=n,t.appendChild(e)),n=n._reactRootContainer,n!=null||t.onclick!==null||(t.onclick=rc));else if(o!==4&&(e=e.child,e!==null))for(a_(e,t,n),e=e.sibling;e!==null;)a_(e,t,n),e=e.sibling}function c_(e,t,n){var o=e.tag;if(o===5||o===6)e=e.stateNode,t?n.insertBefore(e,t):n.appendChild(e);else if(o!==4&&(e=e.child,e!==null))for(c_(e,t,n),e=e.sibling;e!==null;)c_(e,t,n),e=e.sibling}var Zt=null,ao=!1;function er(e,t,n){for(n=n.child;n!==null;)o0(e,t,n),n=n.sibling}function o0(e,t,n){if(Mo&&typeof Mo.onCommitFiberUnmount=="function")try{Mo.onCommitFiberUnmount(vc,n)}catch{}switch(n.tag){case 5:cn||Ni(n,t);case 6:var o=Zt,r=ao;Zt=null,er(e,t,n),Zt=o,ao=r,Zt!==null&&(ao?(e=Zt,n=n.stateNode,e.nodeType===8?e.parentNode.removeChild(n):e.removeChild(n)):Zt.removeChild(n.stateNode));break;case 18:Zt!==null&&(ao?(e=Zt,n=n.stateNode,e.nodeType===8?_d(e.parentNode,n):e.nodeType===1&&_d(e,n),sl(e)):_d(Zt,n.stateNode));break;case 4:o=Zt,r=ao,Zt=n.stateNode.containerInfo,ao=!0,er(e,t,n),Zt=o,ao=r;break;case 0:case 11:case 14:case 15:if(!cn&&(o=n.updateQueue,o!==null&&(o=o.lastEffect,o!==null))){r=o=o.next;do{var i=r,s=i.destroy;i=i.tag,s!==void 0&&((i&2)!==0||(i&4)!==0)&&s_(n,t,s),r=r.next}while(r!==o)}er(e,t,n);break;case 1:if(!cn&&(Ni(n,t),o=n.stateNode,typeof o.componentWillUnmount=="function"))try{o.props=n.memoizedProps,o.state=n.memoizedState,o.componentWillUnmount()}catch(l){Dt(n,t,l)}er(e,t,n);break;case 21:er(e,t,n);break;case 22:n.mode&1?(cn=(o=cn)||n.memoizedState!==null,er(e,t,n),cn=o):er(e,t,n);break;default:er(e,t,n)}}function _g(e){var t=e.updateQueue;if(t!==null){e.updateQueue=null;var n=e.stateNode;n===null&&(n=e.stateNode=new Tx),t.forEach(function(o){var r=Ox.bind(null,e,o);n.has(o)||(n.add(o),o.then(r,r))})}}function so(e,t){var n=t.deletions;if(n!==null)for(var o=0;o<n.length;o++){var r=n[o];try{var i=e,s=t,l=s;e:for(;l!==null;){switch(l.tag){case 5:Zt=l.stateNode,ao=!1;break e;case 3:Zt=l.stateNode.containerInfo,ao=!0;break e;case 4:Zt=l.stateNode.containerInfo,ao=!0;break e}l=l.return}if(Zt===null)throw Error(Q(160));o0(i,s,r),Zt=null,ao=!1;var a=r.alternate;a!==null&&(a.return=null),r.return=null}catch(c){Dt(r,t,c)}}if(t.subtreeFlags&12854)for(t=t.child;t!==null;)r0(t,e),t=t.sibling}function r0(e,t){var n=e.alternate,o=e.flags;switch(e.tag){case 0:case 11:case 14:case 15:if(so(t,e),So(e),o&4){try{Js(3,e,e.return),Tc(3,e)}catch(C){Dt(e,e.return,C)}try{Js(5,e,e.return)}catch(C){Dt(e,e.return,C)}}break;case 1:so(t,e),So(e),o&512&&n!==null&&Ni(n,n.return);break;case 5:if(so(t,e),So(e),o&512&&n!==null&&Ni(n,n.return),e.flags&32){var r=e.stateNode;try{nl(r,"")}catch(C){Dt(e,e.return,C)}}if(o&4&&(r=e.stateNode,r!=null)){var i=e.memoizedProps,s=n!==null?n.memoizedProps:i,l=e.type,a=e.updateQueue;if(e.updateQueue=null,a!==null)try{l==="input"&&i.type==="radio"&&i.name!=null&&Eg(r,i),Id(l,s);var c=Id(l,i);for(s=0;s<a.length;s+=2){var d=a[s],m=a[s+1];d==="style"?Pg(r,m):d==="dangerouslySetInnerHTML"?Lg(r,m):d==="children"?nl(r,m):g_(r,d,m,c)}switch(l){case"input":Td(r,i);break;case"textarea":Mg(r,i);break;case"select":var f=r._wrapperState.wasMultiple;r._wrapperState.wasMultiple=!!i.multiple;var k=i.value;k!=null?Di(r,!!i.multiple,k,!1):f!==!!i.multiple&&(i.defaultValue!=null?Di(r,!!i.multiple,i.defaultValue,!0):Di(r,!!i.multiple,i.multiple?[]:"",!1))}r[dl]=i}catch(C){Dt(e,e.return,C)}}break;case 6:if(so(t,e),So(e),o&4){if(e.stateNode===null)throw Error(Q(162));r=e.stateNode,i=e.memoizedProps;try{r.nodeValue=i}catch(C){Dt(e,e.return,C)}}break;case 3:if(so(t,e),So(e),o&4&&n!==null&&n.memoizedState.isDehydrated)try{sl(t.containerInfo)}catch(C){Dt(e,e.return,C)}break;case 4:so(t,e),So(e);break;case 13:so(t,e),So(e),r=e.child,r.flags&8192&&(i=r.memoizedState!==null,r.stateNode.isHidden=i,!i||r.alternate!==null&&r.alternate.memoizedState!==null||(G_=At())),o&4&&_g(e);break;case 22:if(d=n!==null&&n.memoizedState!==null,e.mode&1?(cn=(c=cn)||d,so(t,e),cn=c):so(t,e),So(e),o&8192){if(c=e.memoizedState!==null,(e.stateNode.isHidden=c)&&!d&&(e.mode&1)!==0)for(se=e,d=e.child;d!==null;){for(m=se=d;se!==null;){switch(f=se,k=f.child,f.tag){case 0:case 11:case 14:case 15:Js(4,f,f.return);break;case 1:Ni(f,f.return);var w=f.stateNode;if(typeof w.componentWillUnmount=="function"){o=f,n=f.return;try{t=o,w.props=t.memoizedProps,w.state=t.memoizedState,w.componentWillUnmount()}catch(C){Dt(o,n,C)}}break;case 5:Ni(f,f.return);break;case 22:if(f.memoizedState!==null){pg(m);continue}}k!==null?(k.return=f,se=k):pg(m)}d=d.sibling}e:for(d=null,m=e;;){if(m.tag===5){if(d===null){d=m;try{r=m.stateNode,c?(i=r.style,typeof i.setProperty=="function"?i.setProperty("display","none","important"):i.display="none"):(l=m.stateNode,a=m.memoizedProps.style,s=a!=null&&a.hasOwnProperty("display")?a.display:null,l.style.display=Rg("display",s))}catch(C){Dt(e,e.return,C)}}}else if(m.tag===6){if(d===null)try{m.stateNode.nodeValue=c?"":m.memoizedProps}catch(C){Dt(e,e.return,C)}}else if((m.tag!==22&&m.tag!==23||m.memoizedState===null||m===e)&&m.child!==null){m.child.return=m,m=m.child;continue}if(m===e)break e;for(;m.sibling===null;){if(m.return===null||m.return===e)break e;d===m&&(d=null),m=m.return}d===m&&(d=null),m.sibling.return=m.return,m=m.sibling}}break;case 19:so(t,e),So(e),o&4&&_g(e);break;case 21:break;default:so(t,e),So(e)}}function So(e){var t=e.flags;if(t&2){try{e:{for(var n=e.return;n!==null;){if(n0(n)){var o=n;break e}n=n.return}throw Error(Q(160))}switch(o.tag){case 5:var r=o.stateNode;o.flags&32&&(nl(r,""),o.flags&=-33);var i=dg(e);c_(e,i,r);break;case 3:case 4:var s=o.stateNode.containerInfo,l=dg(e);a_(e,l,s);break;default:throw Error(Q(161))}}catch(a){Dt(e,e.return,a)}e.flags&=-3}t&4096&&(e.flags&=-4097)}function Rx(e,t,n){se=e,i0(e,t,n)}function i0(e,t,n){for(var o=(e.mode&1)!==0;se!==null;){var r=se,i=r.child;if(r.tag===22&&o){var s=r.memoizedState!==null||Oa;if(!s){var l=r.alternate,a=l!==null&&l.memoizedState!==null||cn;l=Oa;var c=cn;if(Oa=s,(cn=a)&&!c)for(se=r;se!==null;)s=se,a=s.child,s.tag===22&&s.memoizedState!==null?fg(r):a!==null?(a.return=s,se=a):fg(r);for(;i!==null;)se=i,i0(i,t,n),i=i.sibling;se=r,Oa=l,cn=c}mg(e,t,n)}else(r.subtreeFlags&8772)!==0&&i!==null?(i.return=r,se=i):mg(e,t,n)}}function mg(e){for(;se!==null;){var t=se;if((t.flags&8772)!==0){var n=t.alternate;try{if((t.flags&8772)!==0)switch(t.tag){case 0:case 11:case 15:cn||Tc(5,t);break;case 1:var o=t.stateNode;if(t.flags&4&&!cn)if(n===null)o.componentDidMount();else{var r=t.elementType===t.type?n.memoizedProps:lo(t.type,n.memoizedProps);o.componentDidUpdate(r,n.memoizedState,o.__reactInternalSnapshotBeforeUpdate)}var i=t.updateQueue;i!==null&&Kf(t,i,o);break;case 3:var s=t.updateQueue;if(s!==null){if(n=null,t.child!==null)switch(t.child.tag){case 5:n=t.child.stateNode;break;case 1:n=t.child.stateNode}Kf(t,s,n)}break;case 5:var l=t.stateNode;if(n===null&&t.flags&4){n=l;var a=t.memoizedProps;switch(t.type){case"button":case"input":case"select":case"textarea":a.autoFocus&&n.focus();break;case"img":a.src&&(n.src=a.src)}}break;case 6:break;case 4:break;case 12:break;case 13:if(t.memoizedState===null){var c=t.alternate;if(c!==null){var d=c.memoizedState;if(d!==null){var m=d.dehydrated;m!==null&&sl(m)}}}break;case 19:case 17:case 21:case 22:case 23:case 25:break;default:throw Error(Q(163))}cn||t.flags&512&&l_(t)}catch(f){Dt(t,t.return,f)}}if(t===e){se=null;break}if(n=t.sibling,n!==null){n.return=t.return,se=n;break}se=t.return}}function pg(e){for(;se!==null;){var t=se;if(t===e){se=null;break}var n=t.sibling;if(n!==null){n.return=t.return,se=n;break}se=t.return}}function fg(e){for(;se!==null;){var t=se;try{switch(t.tag){case 0:case 11:case 15:var n=t.return;try{Tc(4,t)}catch(a){Dt(t,n,a)}break;case 1:var o=t.stateNode;if(typeof o.componentDidMount=="function"){var r=t.return;try{o.componentDidMount()}catch(a){Dt(t,r,a)}}var i=t.return;try{l_(t)}catch(a){Dt(t,i,a)}break;case 5:var s=t.return;try{l_(t)}catch(a){Dt(t,s,a)}}}catch(a){Dt(t,t.return,a)}if(t===e){se=null;break}var l=t.sibling;if(l!==null){l.return=t.return,se=l;break}se=t.return}}var Px=Math.ceil,fc=Vo.ReactCurrentDispatcher,X_=Vo.ReactCurrentOwner,Xn=Vo.ReactCurrentBatchConfig,Je=0,Xt=null,zt=null,en=0,Pn=0,Ii=yr(0),Ut=0,hl=null,ei=0,Lc=0,Q_=0,Zs=null,kn=null,G_=0,Vi=1/0,Fo=null,gc=!1,u_=null,_r=null,Ba=!1,sr=null,hc=0,el=0,d_=null,Xa=-1,Qa=0;function gn(){return(Je&6)!==0?At():Xa!==-1?Xa:Xa=At()}function mr(e){return(e.mode&1)===0?1:(Je&2)!==0&&en!==0?en&-en:px.transition!==null?(Qa===0&&(Qa=Ug()),Qa):(e=mt,e!==0||(e=window.event,e=e===void 0?16:qg(e.type)),e)}function _o(e,t,n,o){if(50<el)throw el=0,d_=null,Error(Q(185));yl(e,n,o),((Je&2)===0||e!==Xt)&&(e===Xt&&((Je&2)===0&&(Lc|=n),Ut===4&&rr(e,en)),Mn(e,o),n===1&&Je===0&&(t.mode&1)===0&&(Vi=At()+500,Cc&&xr()))}function Mn(e,t){var n=e.callbackNode;g2(e,t);var o=ec(e,e===Xt?en:0);if(o===0)n!==null&&kf(n),e.callbackNode=null,e.callbackPriority=0;else if(t=o&-o,e.callbackPriority!==t){if(n!=null&&kf(n),t===1)e.tag===0?mx(gg.bind(null,e)):fh(gg.bind(null,e)),cx(function(){(Je&6)===0&&xr()}),n=null;else{switch(jg(o)){case 1:n=w_;break;case 4:n=Hg;break;case 16:n=Za;break;case 536870912:n=Wg;break;default:n=Za}n=m0(n,s0.bind(null,e))}e.callbackPriority=t,e.callbackNode=n}}function s0(e,t){if(Xa=-1,Qa=0,(Je&6)!==0)throw Error(Q(327));var n=e.callbackNode;if(Bi()&&e.callbackNode!==n)return null;var o=ec(e,e===Xt?en:0);if(o===0)return null;if((o&30)!==0||(o&e.expiredLanes)!==0||t)t=yc(e,o);else{t=o;var r=Je;Je|=2;var i=a0();(Xt!==e||en!==t)&&(Fo=null,Vi=At()+500,Gr(e,t));do try{Dx();break}catch(l){l0(e,l)}while(!0);D_(),fc.current=i,Je=r,zt!==null?t=0:(Xt=null,en=0,t=Ut)}if(t!==0){if(t===2&&(r=Od(e),r!==0&&(o=r,t=__(e,r))),t===1)throw n=hl,Gr(e,0),rr(e,o),Mn(e,At()),n;if(t===6)rr(e,o);else{if(r=e.current.alternate,(o&30)===0&&!Nx(r)&&(t=yc(e,o),t===2&&(i=Od(e),i!==0&&(o=i,t=__(e,i))),t===1))throw n=hl,Gr(e,0),rr(e,o),Mn(e,At()),n;switch(e.finishedWork=r,e.finishedLanes=o,t){case 0:case 1:throw Error(Q(345));case 2:Yr(e,kn,Fo);break;case 3:if(rr(e,o),(o&130023424)===o&&(t=G_+500-At(),10<t)){if(ec(e,0)!==0)break;if(r=e.suspendedLanes,(r&o)!==o){gn(),e.pingedLanes|=e.suspendedLanes&r;break}e.timeoutHandle=Vd(Yr.bind(null,e,kn,Fo),t);break}Yr(e,kn,Fo);break;case 4:if(rr(e,o),(o&4194240)===o)break;for(t=e.eventTimes,r=-1;0<o;){var s=31-uo(o);i=1<<s,s=t[s],s>r&&(r=s),o&=~i}if(o=r,o=At()-o,o=(120>o?120:480>o?480:1080>o?1080:1920>o?1920:3e3>o?3e3:4320>o?4320:1960*Px(o/1960))-o,10<o){e.timeoutHandle=Vd(Yr.bind(null,e,kn,Fo),o);break}Yr(e,kn,Fo);break;case 5:Yr(e,kn,Fo);break;default:throw Error(Q(329))}}}return Mn(e,At()),e.callbackNode===n?s0.bind(null,e):null}function __(e,t){var n=Zs;return e.current.memoizedState.isDehydrated&&(Gr(e,t).flags|=256),e=yc(e,t),e!==2&&(t=kn,kn=n,t!==null&&m_(t)),e}function m_(e){kn===null?kn=e:kn.push.apply(kn,e)}function Nx(e){for(var t=e;;){if(t.flags&16384){var n=t.updateQueue;if(n!==null&&(n=n.stores,n!==null))for(var o=0;o<n.length;o++){var r=n[o],i=r.getSnapshot;r=r.value;try{if(!mo(i(),r))return!1}catch{return!1}}}if(n=t.child,t.subtreeFlags&16384&&n!==null)n.return=t,t=n;else{if(t===e)break;for(;t.sibling===null;){if(t.return===null||t.return===e)return!0;t=t.return}t.sibling.return=t.return,t=t.sibling}}return!0}function rr(e,t){for(t&=~Q_,t&=~Lc,e.suspendedLanes|=t,e.pingedLanes&=~t,e=e.expirationTimes;0<t;){var n=31-uo(t),o=1<<n;e[n]=-1,t&=~o}}function gg(e){if((Je&6)!==0)throw Error(Q(327));Bi();var t=ec(e,0);if((t&1)===0)return Mn(e,At()),null;var n=yc(e,t);if(e.tag!==0&&n===2){var o=Od(e);o!==0&&(t=o,n=__(e,o))}if(n===1)throw n=hl,Gr(e,0),rr(e,t),Mn(e,At()),n;if(n===6)throw Error(Q(345));return e.finishedWork=e.current.alternate,e.finishedLanes=t,Yr(e,kn,Fo),Mn(e,At()),null}function q_(e,t){var n=Je;Je|=1;try{return e(t)}finally{Je=n,Je===0&&(Vi=At()+500,Cc&&xr())}}function ti(e){sr!==null&&sr.tag===0&&(Je&6)===0&&Bi();var t=Je;Je|=1;var n=Xn.transition,o=mt;try{if(Xn.transition=null,mt=1,e)return e()}finally{mt=o,Xn.transition=n,Je=t,(Je&6)===0&&xr()}}function K_(){Pn=Ii.current,Ct(Ii)}function Gr(e,t){e.finishedWork=null,e.finishedLanes=0;var n=e.timeoutHandle;if(n!==-1&&(e.timeoutHandle=-1,ax(n)),zt!==null)for(n=zt.return;n!==null;){var o=n;switch(P_(o),o.tag){case 1:o=o.type.childContextTypes,o!=null&&ic();break;case 3:ji(),Ct(Cn),Ct(un),z_();break;case 5:B_(o);break;case 4:ji();break;case 13:Ct(Lt);break;case 19:Ct(Lt);break;case 10:$_(o.type._context);break;case 22:case 23:K_()}n=n.return}if(Xt=e,zt=e=pr(e.current,null),en=Pn=t,Ut=0,hl=null,Q_=Lc=ei=0,kn=Zs=null,Xr!==null){for(t=0;t<Xr.length;t++)if(n=Xr[t],o=n.interleaved,o!==null){n.interleaved=null;var r=o.next,i=n.pending;if(i!==null){var s=i.next;i.next=r,o.next=s}n.pending=o}Xr=null}return e}function l0(e,t){do{var n=zt;try{if(D_(),ja.current=pc,mc){for(var o=Rt.memoizedState;o!==null;){var r=o.queue;r!==null&&(r.pending=null),o=o.next}mc=!1}if(Zr=0,Vt=Wt=Rt=null,Ks=!1,pl=0,X_.current=null,n===null||n.return===null){Ut=1,hl=t,zt=null;break}e:{var i=e,s=n.return,l=n,a=t;if(t=en,l.flags|=32768,a!==null&&typeof a=="object"&&typeof a.then=="function"){var c=a,d=l,m=d.tag;if((d.mode&1)===0&&(m===0||m===11||m===15)){var f=d.alternate;f?(d.updateQueue=f.updateQueue,d.memoizedState=f.memoizedState,d.lanes=f.lanes):(d.updateQueue=null,d.memoizedState=null)}var k=og(s);if(k!==null){k.flags&=-257,rg(k,s,l,i,t),k.mode&1&&ng(i,c,t),t=k,a=c;var w=t.updateQueue;if(w===null){var C=new Set;C.add(a),t.updateQueue=C}else w.add(a);break e}else{if((t&1)===0){ng(i,c,t),J_();break e}a=Error(Q(426))}}else if(Tt&&l.mode&1){var S=og(s);if(S!==null){(S.flags&65536)===0&&(S.flags|=256),rg(S,s,l,i,t),N_(Yi(a,l));break e}}i=a=Yi(a,l),Ut!==4&&(Ut=2),Zs===null?Zs=[i]:Zs.push(i),i=s;do{switch(i.tag){case 3:i.flags|=65536,t&=-t,i.lanes|=t;var g=jh(i,a,t);qf(i,g);break e;case 1:l=a;var x=i.type,h=i.stateNode;if((i.flags&128)===0&&(typeof x.getDerivedStateFromError=="function"||h!==null&&typeof h.componentDidCatch=="function"&&(_r===null||!_r.has(h)))){i.flags|=65536,t&=-t,i.lanes|=t;var E=Yh(i,l,t);qf(i,E);break e}}i=i.return}while(i!==null)}u0(n)}catch(F){t=F,zt===n&&n!==null&&(zt=n=n.return);continue}break}while(!0)}function a0(){var e=fc.current;return fc.current=pc,e===null?pc:e}function J_(){(Ut===0||Ut===3||Ut===2)&&(Ut=4),Xt===null||(ei&268435455)===0&&(Lc&268435455)===0||rr(Xt,en)}function yc(e,t){var n=Je;Je|=2;var o=a0();(Xt!==e||en!==t)&&(Fo=null,Gr(e,t));do try{Ix();break}catch(r){l0(e,r)}while(!0);if(D_(),Je=n,fc.current=o,zt!==null)throw Error(Q(261));return Xt=null,en=0,Ut}function Ix(){for(;zt!==null;)c0(zt)}function Dx(){for(;zt!==null&&!l2();)c0(zt)}function c0(e){var t=_0(e.alternate,e,Pn);e.memoizedProps=e.pendingProps,t===null?u0(e):zt=t,X_.current=null}function u0(e){var t=e;do{var n=t.alternate;if(e=t.return,(t.flags&32768)===0){if(n=Ex(n,t,Pn),n!==null){zt=n;return}}else{if(n=Mx(n,t),n!==null){n.flags&=32767,zt=n;return}if(e!==null)e.flags|=32768,e.subtreeFlags=0,e.deletions=null;else{Ut=6,zt=null;return}}if(t=t.sibling,t!==null){zt=t;return}zt=t=e}while(t!==null);Ut===0&&(Ut=5)}function Yr(e,t,n){var o=mt,r=Xn.transition;try{Xn.transition=null,mt=1,$x(e,t,n,o)}finally{Xn.transition=r,mt=o}return null}function $x(e,t,n,o){do Bi();while(sr!==null);if((Je&6)!==0)throw Error(Q(327));n=e.finishedWork;var r=e.finishedLanes;if(n===null)return null;if(e.finishedWork=null,e.finishedLanes=0,n===e.current)throw Error(Q(177));e.callbackNode=null,e.callbackPriority=0;var i=n.lanes|n.childLanes;if(h2(e,i),e===Xt&&(zt=Xt=null,en=0),(n.subtreeFlags&2064)===0&&(n.flags&2064)===0||Ba||(Ba=!0,m0(Za,function(){return Bi(),null})),i=(n.flags&15990)!==0,(n.subtreeFlags&15990)!==0||i){i=Xn.transition,Xn.transition=null;var s=mt;mt=1;var l=Je;Je|=4,X_.current=null,Lx(e,n),r0(n,e),ox(jd),tc=!!Ud,jd=Ud=null,e.current=n,Rx(n,e,r),a2(),Je=l,mt=s,Xn.transition=i}else e.current=n;if(Ba&&(Ba=!1,sr=e,hc=r),i=e.pendingLanes,i===0&&(_r=null),d2(n.stateNode,o),Mn(e,At()),t!==null)for(o=e.onRecoverableError,n=0;n<t.length;n++)r=t[n],o(r.value,{componentStack:r.stack,digest:r.digest});if(gc)throw gc=!1,e=u_,u_=null,e;return(hc&1)!==0&&e.tag!==0&&Bi(),i=e.pendingLanes,(i&1)!==0?e===d_?el++:(el=0,d_=e):el=0,xr(),null}function Bi(){if(sr!==null){var e=jg(hc),t=Xn.transition,n=mt;try{if(Xn.transition=null,mt=16>e?16:e,sr===null)var o=!1;else{if(e=sr,sr=null,hc=0,(Je&6)!==0)throw Error(Q(331));var r=Je;for(Je|=4,se=e.current;se!==null;){var i=se,s=i.child;if((se.flags&16)!==0){var l=i.deletions;if(l!==null){for(var a=0;a<l.length;a++){var c=l[a];for(se=c;se!==null;){var d=se;switch(d.tag){case 0:case 11:case 15:Js(8,d,i)}var m=d.child;if(m!==null)m.return=d,se=m;else for(;se!==null;){d=se;var f=d.sibling,k=d.return;if(t0(d),d===c){se=null;break}if(f!==null){f.return=k,se=f;break}se=k}}}var w=i.alternate;if(w!==null){var C=w.child;if(C!==null){w.child=null;do{var S=C.sibling;C.sibling=null,C=S}while(C!==null)}}se=i}}if((i.subtreeFlags&2064)!==0&&s!==null)s.return=i,se=s;else e:for(;se!==null;){if(i=se,(i.flags&2048)!==0)switch(i.tag){case 0:case 11:case 15:Js(9,i,i.return)}var g=i.sibling;if(g!==null){g.return=i.return,se=g;break e}se=i.return}}var x=e.current;for(se=x;se!==null;){s=se;var h=s.child;if((s.subtreeFlags&2064)!==0&&h!==null)h.return=s,se=h;else e:for(s=x;se!==null;){if(l=se,(l.flags&2048)!==0)try{switch(l.tag){case 0:case 11:case 15:Tc(9,l)}}catch(F){Dt(l,l.return,F)}if(l===s){se=null;break e}var E=l.sibling;if(E!==null){E.return=l.return,se=E;break e}se=l.return}}if(Je=r,xr(),Mo&&typeof Mo.onPostCommitFiberRoot=="function")try{Mo.onPostCommitFiberRoot(vc,e)}catch{}o=!0}return o}finally{mt=n,Xn.transition=t}}return!1}function hg(e,t,n){t=Yi(n,t),t=jh(e,t,1),e=dr(e,t,1),t=gn(),e!==null&&(yl(e,1,t),Mn(e,t))}function Dt(e,t,n){if(e.tag===3)hg(e,e,n);else for(;t!==null;){if(t.tag===3){hg(t,e,n);break}else if(t.tag===1){var o=t.stateNode;if(typeof t.type.getDerivedStateFromError=="function"||typeof o.componentDidCatch=="function"&&(_r===null||!_r.has(o))){e=Yi(n,e),e=Yh(t,e,1),t=dr(t,e,1),e=gn(),t!==null&&(yl(t,1,e),Mn(t,e));break}}t=t.return}}function Ax(e,t,n){var o=e.pingCache;o!==null&&o.delete(t),t=gn(),e.pingedLanes|=e.suspendedLanes&n,Xt===e&&(en&n)===n&&(Ut===4||Ut===3&&(en&130023424)===en&&500>At()-G_?Gr(e,0):Q_|=n),Mn(e,t)}function d0(e,t){t===0&&((e.mode&1)===0?t=1:(t=Ca,Ca<<=1,(Ca&130023424)===0&&(Ca=4194304)));var n=gn();e=jo(e,t),e!==null&&(yl(e,t,n),Mn(e,n))}function Fx(e){var t=e.memoizedState,n=0;t!==null&&(n=t.retryLane),d0(e,n)}function Ox(e,t){var n=0;switch(e.tag){case 13:var o=e.stateNode,r=e.memoizedState;r!==null&&(n=r.retryLane);break;case 19:o=e.stateNode;break;default:throw Error(Q(314))}o!==null&&o.delete(t),d0(e,n)}var _0;_0=function(e,t,n){if(e!==null)if(e.memoizedProps!==t.pendingProps||Cn.current)Sn=!0;else{if((e.lanes&n)===0&&(t.flags&128)===0)return Sn=!1,Cx(e,t,n);Sn=(e.flags&131072)!==0}else Sn=!1,Tt&&(t.flags&1048576)!==0&&gh(t,ac,t.index);switch(t.lanes=0,t.tag){case 2:var o=t.type;Va(e,t),e=t.pendingProps;var r=Hi(t,un.current);Oi(t,n),r=W_(null,t,o,e,r,n);var i=U_();return t.flags|=1,typeof r=="object"&&r!==null&&typeof r.render=="function"&&r.$$typeof===void 0?(t.tag=1,t.memoizedState=null,t.updateQueue=null,En(o)?(i=!0,sc(t)):i=!1,t.memoizedState=r.state!==null&&r.state!==void 0?r.state:null,F_(t),r.updater=Mc,t.stateNode=r,r._reactInternals=t,Zd(t,o,e,n),t=n_(null,t,o,!0,i,n)):(t.tag=0,Tt&&i&&R_(t),fn(null,t,r,n),t=t.child),t;case 16:o=t.elementType;e:{switch(Va(e,t),e=t.pendingProps,r=o._init,o=r(o._payload),t.type=o,r=t.tag=zx(o),e=lo(o,e),r){case 0:t=t_(null,t,o,e,n);break e;case 1:t=lg(null,t,o,e,n);break e;case 11:t=ig(null,t,o,e,n);break e;case 14:t=sg(null,t,o,lo(o.type,e),n);break e}throw Error(Q(306,o,""))}return t;case 0:return o=t.type,r=t.pendingProps,r=t.elementType===o?r:lo(o,r),t_(e,t,o,r,n);case 1:return o=t.type,r=t.pendingProps,r=t.elementType===o?r:lo(o,r),lg(e,t,o,r,n);case 3:e:{if(Gh(t),e===null)throw Error(Q(387));o=t.pendingProps,i=t.memoizedState,r=i.element,bh(e,t),dc(t,o,null,n);var s=t.memoizedState;if(o=s.element,i.isDehydrated)if(i={element:o,isDehydrated:!1,cache:s.cache,pendingSuspenseBoundaries:s.pendingSuspenseBoundaries,transitions:s.transitions},t.updateQueue.baseState=i,t.memoizedState=i,t.flags&256){r=Yi(Error(Q(423)),t),t=ag(e,t,o,n,r);break e}else if(o!==r){r=Yi(Error(Q(424)),t),t=ag(e,t,o,n,r);break e}else for(Nn=ur(t.stateNode.containerInfo.firstChild),In=t,Tt=!0,co=null,n=vh(t,null,o,n),t.child=n;n;)n.flags=n.flags&-3|4096,n=n.sibling;else{if(Wi(),o===r){t=Yo(e,t,n);break e}fn(e,t,o,n)}t=t.child}return t;case 5:return kh(t),e===null&&qd(t),o=t.type,r=t.pendingProps,i=e!==null?e.memoizedProps:null,s=r.children,Yd(o,r)?s=null:i!==null&&Yd(o,i)&&(t.flags|=32),Qh(e,t),fn(e,t,s,n),t.child;case 6:return e===null&&qd(t),null;case 13:return qh(e,t,n);case 4:return O_(t,t.stateNode.containerInfo),o=t.pendingProps,e===null?t.child=Ui(t,null,o,n):fn(e,t,o,n),t.child;case 11:return o=t.type,r=t.pendingProps,r=t.elementType===o?r:lo(o,r),ig(e,t,o,r,n);case 7:return fn(e,t,t.pendingProps,n),t.child;case 8:return fn(e,t,t.pendingProps.children,n),t.child;case 12:return fn(e,t,t.pendingProps.children,n),t.child;case 10:e:{if(o=t.type._context,r=t.pendingProps,i=t.memoizedProps,s=r.value,bt(cc,o._currentValue),o._currentValue=s,i!==null)if(mo(i.value,s)){if(i.children===r.children&&!Cn.current){t=Yo(e,t,n);break e}}else for(i=t.child,i!==null&&(i.return=t);i!==null;){var l=i.dependencies;if(l!==null){s=i.child;for(var a=l.firstContext;a!==null;){if(a.context===o){if(i.tag===1){a=Ho(-1,n&-n),a.tag=2;var c=i.updateQueue;if(c!==null){c=c.shared;var d=c.pending;d===null?a.next=a:(a.next=d.next,d.next=a),c.pending=a}}i.lanes|=n,a=i.alternate,a!==null&&(a.lanes|=n),Kd(i.return,n,t),l.lanes|=n;break}a=a.next}}else if(i.tag===10)s=i.type===t.type?null:i.child;else if(i.tag===18){if(s=i.return,s===null)throw Error(Q(341));s.lanes|=n,l=s.alternate,l!==null&&(l.lanes|=n),Kd(s,n,t),s=i.sibling}else s=i.child;if(s!==null)s.return=i;else for(s=i;s!==null;){if(s===t){s=null;break}if(i=s.sibling,i!==null){i.return=s.return,s=i;break}s=s.return}i=s}fn(e,t,r.children,n),t=t.child}return t;case 9:return r=t.type,o=t.pendingProps.children,Oi(t,n),r=Qn(r),o=o(r),t.flags|=1,fn(e,t,o,n),t.child;case 14:return o=t.type,r=lo(o,t.pendingProps),r=lo(o.type,r),sg(e,t,o,r,n);case 15:return Vh(e,t,t.type,t.pendingProps,n);case 17:return o=t.type,r=t.pendingProps,r=t.elementType===o?r:lo(o,r),Va(e,t),t.tag=1,En(o)?(e=!0,sc(t)):e=!1,Oi(t,n),Uh(t,o,r),Zd(t,o,r,n),n_(null,t,o,!0,e,n);case 19:return Kh(e,t,n);case 22:return Xh(e,t,n)}throw Error(Q(156,t.tag))};function m0(e,t){return zg(e,t)}function Bx(e,t,n,o){this.tag=e,this.key=n,this.sibling=this.child=this.return=this.stateNode=this.type=this.elementType=null,this.index=0,this.ref=null,this.pendingProps=t,this.dependencies=this.memoizedState=this.updateQueue=this.memoizedProps=null,this.mode=o,this.subtreeFlags=this.flags=0,this.deletions=null,this.childLanes=this.lanes=0,this.alternate=null}function Vn(e,t,n,o){return new Bx(e,t,n,o)}function Z_(e){return e=e.prototype,!(!e||!e.isReactComponent)}function zx(e){if(typeof e=="function")return Z_(e)?1:0;if(e!=null){if(e=e.$$typeof,e===y_)return 11;if(e===x_)return 14}return 2}function pr(e,t){var n=e.alternate;return n===null?(n=Vn(e.tag,t,e.key,e.mode),n.elementType=e.elementType,n.type=e.type,n.stateNode=e.stateNode,n.alternate=e,e.alternate=n):(n.pendingProps=t,n.type=e.type,n.flags=0,n.subtreeFlags=0,n.deletions=null),n.flags=e.flags&14680064,n.childLanes=e.childLanes,n.lanes=e.lanes,n.child=e.child,n.memoizedProps=e.memoizedProps,n.memoizedState=e.memoizedState,n.updateQueue=e.updateQueue,t=e.dependencies,n.dependencies=t===null?null:{lanes:t.lanes,firstContext:t.firstContext},n.sibling=e.sibling,n.index=e.index,n.ref=e.ref,n}function Ga(e,t,n,o,r,i){var s=2;if(o=e,typeof e=="function")Z_(e)&&(s=1);else if(typeof e=="string")s=5;else e:switch(e){case ki:return qr(n.children,r,i,t);case h_:s=8,r|=8;break;case kd:return e=Vn(12,n,t,r|2),e.elementType=kd,e.lanes=i,e;case Sd:return e=Vn(13,n,t,r),e.elementType=Sd,e.lanes=i,e;case Cd:return e=Vn(19,n,t,r),e.elementType=Cd,e.lanes=i,e;case kg:return Rc(n,r,i,t);default:if(typeof e=="object"&&e!==null)switch(e.$$typeof){case wg:s=10;break e;case bg:s=9;break e;case y_:s=11;break e;case x_:s=14;break e;case tr:s=16,o=null;break e}throw Error(Q(130,e==null?e:typeof e,""))}return t=Vn(s,n,t,r),t.elementType=e,t.type=o,t.lanes=i,t}function qr(e,t,n,o){return e=Vn(7,e,o,t),e.lanes=n,e}function Rc(e,t,n,o){return e=Vn(22,e,o,t),e.elementType=kg,e.lanes=n,e.stateNode={isHidden:!1},e}function vd(e,t,n){return e=Vn(6,e,null,t),e.lanes=n,e}function wd(e,t,n){return t=Vn(4,e.children!==null?e.children:[],e.key,t),t.lanes=n,t.stateNode={containerInfo:e.containerInfo,pendingChildren:null,implementation:e.implementation},t}function Hx(e,t,n,o,r){this.tag=t,this.containerInfo=e,this.finishedWork=this.pingCache=this.current=this.pendingChildren=null,this.timeoutHandle=-1,this.callbackNode=this.pendingContext=this.context=null,this.callbackPriority=0,this.eventTimes=rd(0),this.expirationTimes=rd(-1),this.entangledLanes=this.finishedLanes=this.mutableReadLanes=this.expiredLanes=this.pingedLanes=this.suspendedLanes=this.pendingLanes=0,this.entanglements=rd(0),this.identifierPrefix=o,this.onRecoverableError=r,this.mutableSourceEagerHydrationData=null}function em(e,t,n,o,r,i,s,l,a){return e=new Hx(e,t,n,l,a),t===1?(t=1,i===!0&&(t|=8)):t=0,i=Vn(3,null,null,t),e.current=i,i.stateNode=e,i.memoizedState={element:o,isDehydrated:n,cache:null,transitions:null,pendingSuspenseBoundaries:null},F_(i),e}function Wx(e,t,n){var o=3<arguments.length&&arguments[3]!==void 0?arguments[3]:null;return{$$typeof:bi,key:o==null?null:""+o,children:e,containerInfo:t,implementation:n}}function p0(e){if(!e)return gr;e=e._reactInternals;e:{if(oi(e)!==e||e.tag!==1)throw Error(Q(170));var t=e;do{switch(t.tag){case 3:t=t.stateNode.context;break e;case 1:if(En(t.type)){t=t.stateNode.__reactInternalMemoizedMergedChildContext;break e}}t=t.return}while(t!==null);throw Error(Q(171))}if(e.tag===1){var n=e.type;if(En(n))return ph(e,n,t)}return t}function f0(e,t,n,o,r,i,s,l,a){return e=em(n,o,!0,e,r,i,s,l,a),e.context=p0(null),n=e.current,o=gn(),r=mr(n),i=Ho(o,r),i.callback=t??null,dr(n,i,r),e.current.lanes=r,yl(e,r,o),Mn(e,o),e}function Pc(e,t,n,o){var r=t.current,i=gn(),s=mr(r);return n=p0(n),t.context===null?t.context=n:t.pendingContext=n,t=Ho(i,s),t.payload={element:e},o=o===void 0?null:o,o!==null&&(t.callback=o),e=dr(r,t,s),e!==null&&(_o(e,r,s,i),Ua(e,r,s)),s}function xc(e){return e=e.current,e.child?(e.child.tag===5,e.child.stateNode):null}function yg(e,t){if(e=e.memoizedState,e!==null&&e.dehydrated!==null){var n=e.retryLane;e.retryLane=n!==0&&n<t?n:t}}function tm(e,t){yg(e,t),(e=e.alternate)&&yg(e,t)}function Ux(){return null}var g0=typeof reportError=="function"?reportError:function(e){console.error(e)};function nm(e){this._internalRoot=e}Nc.prototype.render=nm.prototype.render=function(e){var t=this._internalRoot;if(t===null)throw Error(Q(409));Pc(e,t,null,null)};Nc.prototype.unmount=nm.prototype.unmount=function(){var e=this._internalRoot;if(e!==null){this._internalRoot=null;var t=e.containerInfo;ti(function(){Pc(null,e,null,null)}),t[Uo]=null}};function Nc(e){this._internalRoot=e}Nc.prototype.unstable_scheduleHydration=function(e){if(e){var t=Xg();e={blockedOn:null,target:e,priority:t};for(var n=0;n<or.length&&t!==0&&t<or[n].priority;n++);or.splice(n,0,e),n===0&&Gg(e)}};function om(e){return!(!e||e.nodeType!==1&&e.nodeType!==9&&e.nodeType!==11)}function Ic(e){return!(!e||e.nodeType!==1&&e.nodeType!==9&&e.nodeType!==11&&(e.nodeType!==8||e.nodeValue!==" react-mount-point-unstable "))}function xg(){}function jx(e,t,n,o,r){if(r){if(typeof o=="function"){var i=o;o=function(){var c=xc(s);i.call(c)}}var s=f0(t,o,e,0,null,!1,!1,"",xg);return e._reactRootContainer=s,e[Uo]=s.current,cl(e.nodeType===8?e.parentNode:e),ti(),s}for(;r=e.lastChild;)e.removeChild(r);if(typeof o=="function"){var l=o;o=function(){var c=xc(a);l.call(c)}}var a=em(e,0,!1,null,null,!1,!1,"",xg);return e._reactRootContainer=a,e[Uo]=a.current,cl(e.nodeType===8?e.parentNode:e),ti(function(){Pc(t,a,n,o)}),a}function Dc(e,t,n,o,r){var i=n._reactRootContainer;if(i){var s=i;if(typeof r=="function"){var l=r;r=function(){var a=xc(s);l.call(a)}}Pc(t,s,e,r)}else s=jx(n,t,e,r,o);return xc(s)}Yg=function(e){switch(e.tag){case 3:var t=e.stateNode;if(t.current.memoizedState.isDehydrated){var n=js(t.pendingLanes);n!==0&&(b_(t,n|1),Mn(t,At()),(Je&6)===0&&(Vi=At()+500,xr()))}break;case 13:ti(function(){var o=jo(e,1);if(o!==null){var r=gn();_o(o,e,1,r)}}),tm(e,1)}};k_=function(e){if(e.tag===13){var t=jo(e,134217728);if(t!==null){var n=gn();_o(t,e,134217728,n)}tm(e,134217728)}};Vg=function(e){if(e.tag===13){var t=mr(e),n=jo(e,t);if(n!==null){var o=gn();_o(n,e,t,o)}tm(e,t)}};Xg=function(){return mt};Qg=function(e,t){var n=mt;try{return mt=e,t()}finally{mt=n}};$d=function(e,t,n){switch(t){case"input":if(Td(e,n),t=n.name,n.type==="radio"&&t!=null){for(n=e;n.parentNode;)n=n.parentNode;for(n=n.querySelectorAll("input[name="+JSON.stringify(""+t)+'][type="radio"]'),t=0;t<n.length;t++){var o=n[t];if(o!==e&&o.form===e.form){var r=Sc(o);if(!r)throw Error(Q(90));Cg(o),Td(o,r)}}}break;case"textarea":Mg(e,n);break;case"select":t=n.value,t!=null&&Di(e,!!n.multiple,t,!1)}};Dg=q_;$g=ti;var Yx={usingClientEntryPoint:!1,Events:[vl,Mi,Sc,Ng,Ig,q_]},zs={findFiberByHostInstance:Vr,bundleType:0,version:"18.3.1",rendererPackageName:"react-dom"},Vx={bundleType:zs.bundleType,version:zs.version,rendererPackageName:zs.rendererPackageName,rendererConfig:zs.rendererConfig,overrideHookState:null,overrideHookStateDeletePath:null,overrideHookStateRenamePath:null,overrideProps:null,overridePropsDeletePath:null,overridePropsRenamePath:null,setErrorHandler:null,setSuspenseHandler:null,scheduleUpdate:null,currentDispatcherRef:Vo.ReactCurrentDispatcher,findHostInstanceByFiber:function(e){return e=Og(e),e===null?null:e.stateNode},findFiberByHostInstance:zs.findFiberByHostInstance||Ux,findHostInstancesForRefresh:null,scheduleRefresh:null,scheduleRoot:null,setRefreshHandler:null,getCurrentFiber:null,reconcilerVersion:"18.3.1-next-f1338f8080-20240426"};if(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__<"u"&&(Hs=__REACT_DEVTOOLS_GLOBAL_HOOK__,!Hs.isDisabled&&Hs.supportsFiber))try{vc=Hs.inject(Vx),Mo=Hs}catch{}var Hs;An.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED=Yx;An.createPortal=function(e,t){var n=2<arguments.length&&arguments[2]!==void 0?arguments[2]:null;if(!om(t))throw Error(Q(200));return Wx(e,t,null,n)};An.createRoot=function(e,t){if(!om(e))throw Error(Q(299));var n=!1,o="",r=g0;return t!=null&&(t.unstable_strictMode===!0&&(n=!0),t.identifierPrefix!==void 0&&(o=t.identifierPrefix),t.onRecoverableError!==void 0&&(r=t.onRecoverableError)),t=em(e,1,!1,null,null,n,!1,o,r),e[Uo]=t.current,cl(e.nodeType===8?e.parentNode:e),new nm(t)};An.findDOMNode=function(e){if(e==null)return null;if(e.nodeType===1)return e;var t=e._reactInternals;if(t===void 0)throw typeof e.render=="function"?Error(Q(188)):(e=Object.keys(e).join(","),Error(Q(268,e)));return e=Og(t),e=e===null?null:e.stateNode,e};An.flushSync=function(e){return ti(e)};An.hydrate=function(e,t,n){if(!Ic(t))throw Error(Q(200));return Dc(null,e,t,!0,n)};An.hydrateRoot=function(e,t,n){if(!om(e))throw Error(Q(405));var o=n!=null&&n.hydratedSources||null,r=!1,i="",s=g0;if(n!=null&&(n.unstable_strictMode===!0&&(r=!0),n.identifierPrefix!==void 0&&(i=n.identifierPrefix),n.onRecoverableError!==void 0&&(s=n.onRecoverableError)),t=f0(t,null,e,1,n??null,r,!1,i,s),e[Uo]=t.current,cl(e),o)for(e=0;e<o.length;e++)n=o[e],r=n._getVersion,r=r(n._source),t.mutableSourceEagerHydrationData==null?t.mutableSourceEagerHydrationData=[n,r]:t.mutableSourceEagerHydrationData.push(n,r);return new Nc(t)};An.render=function(e,t,n){if(!Ic(t))throw Error(Q(200));return Dc(null,e,t,!1,n)};An.unmountComponentAtNode=function(e){if(!Ic(e))throw Error(Q(40));return e._reactRootContainer?(ti(function(){Dc(null,null,e,!1,function(){e._reactRootContainer=null,e[Uo]=null})}),!0):!1};An.unstable_batchedUpdates=q_;An.unstable_renderSubtreeIntoContainer=function(e,t,n,o){if(!Ic(n))throw Error(Q(200));if(e==null||e._reactInternals===void 0)throw Error(Q(38));return Dc(e,t,n,!1,o)};An.version="18.3.1-next-f1338f8080-20240426"});var bl=Ao((Hk,x0)=>{"use strict";function y0(){if(!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__>"u"||typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE!="function"))try{__REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(y0)}catch(e){console.error(e)}}y0(),x0.exports=h0()});var w0=Ao(rm=>{"use strict";var v0=bl();rm.createRoot=v0.createRoot,rm.hydrateRoot=v0.hydrateRoot;var Wk});var W0=Ao(Hc=>{"use strict";var sv=vt(),lv=Symbol.for("react.element"),av=Symbol.for("react.fragment"),cv=Object.prototype.hasOwnProperty,uv=sv.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED.ReactCurrentOwner,dv={key:!0,ref:!0,__self:!0,__source:!0};function H0(e,t,n){var o,r={},i=null,s=null;n!==void 0&&(i=""+n),t.key!==void 0&&(i=""+t.key),t.ref!==void 0&&(s=t.ref);for(o in t)cv.call(t,o)&&!dv.hasOwnProperty(o)&&(r[o]=t[o]);if(e&&e.defaultProps)for(o in t=e.defaultProps,t)r[o]===void 0&&(r[o]=t[o]);return{$$typeof:lv,type:e,key:i,ref:s,props:r,_owner:uv.current}}Hc.Fragment=av;Hc.jsx=H0;Hc.jsxs=H0});var ze=Ao((i4,U0)=>{"use strict";U0.exports=W0()});var Ck={};T5(Ck,{Agentation:()=>qm,createElement:()=>G1.createElement,createRoot:()=>q1.createRoot,version:()=>Sk});var G1=ce(vt()),q1=ce(w0());var Ac=ce(vt());var b0=["data-feedback-toolbar","data-annotation-popup","data-annotation-marker"],im=b0.flatMap(e=>[`:not([${e}])`,`:not([${e}] *)`]).join(""),sm="feedback-freeze-styles",lm="__agentation_freeze";function Xx(){return typeof window>"u"?{frozen:!1,installed:!0,origSetTimeout:setTimeout,origSetInterval:setInterval,origRAF:t=>0,pausedAnimations:[],frozenTimeoutQueue:[],frozenRAFQueue:[]}:window[lm]??{frozen:!1,installed:!1,origSetTimeout:window.setTimeout.bind(window),origSetInterval:window.setInterval.bind(window),origRAF:window.requestAnimationFrame.bind(window),pausedAnimations:[],frozenTimeoutQueue:[],frozenRAFQueue:[]}}var Ue=Xx();function am(){if(typeof window>"u")return;let e=window;Ue=e[lm]??(e[lm]=Ue),!Ue.installed&&(window.setTimeout=(t,n,...o)=>typeof t=="string"?Ue.origSetTimeout(t,n):Ue.origSetTimeout((...r)=>{Ue.frozen?Ue.frozenTimeoutQueue.push(()=>t(...r)):t(...r)},n,...o),window.setInterval=(t,n,...o)=>typeof t=="string"?Ue.origSetInterval(t,n):Ue.origSetInterval((...r)=>{Ue.frozen||t(...r)},n,...o),window.requestAnimationFrame=t=>Ue.origRAF(n=>{Ue.frozen?Ue.frozenRAFQueue.push(t):t(n)}),Ue.installed=!0)}var De=Ue.origSetTimeout,$c=Ue.origSetInterval,Gi=Ue.origRAF;function Qx(e){return e?b0.some(t=>!!e.closest?.(`[${t}]`)):!1}function k0(){if(typeof document>"u"||(am(),Ue.frozen))return;Ue.frozen=!0,Ue.frozenTimeoutQueue=[],Ue.frozenRAFQueue=[];let e=document.getElementById(sm);e||(e=document.createElement("style"),e.id=sm),e.textContent=`
    *${im},
    *${im}::before,
    *${im}::after {
      animation-play-state: paused !important;
      transition: none !important;
    }
  `,document.head.appendChild(e),Ue.pausedAnimations=[];try{document.getAnimations().forEach(t=>{if(t.playState!=="running")return;let n=t.effect?.target;Qx(n)||(t.pause(),Ue.pausedAnimations.push(t))})}catch{}document.querySelectorAll("video").forEach(t=>{t.paused||(t.dataset.wasPaused="false",t.pause())})}function cm(){if(typeof document>"u"||!Ue.frozen)return;Ue.frozen=!1;let e=Ue.frozenTimeoutQueue;Ue.frozenTimeoutQueue=[];for(let n of e)Ue.origSetTimeout(()=>{if(Ue.frozen){Ue.frozenTimeoutQueue.push(n);return}try{n()}catch(o){console.warn("[agentation] Error replaying queued timeout:",o)}},0);let t=Ue.frozenRAFQueue;Ue.frozenRAFQueue=[];for(let n of t)Ue.origRAF(o=>{if(Ue.frozen){Ue.frozenRAFQueue.push(n);return}n(o)});for(let n of Ue.pausedAnimations)try{n.play()}catch(o){console.warn("[agentation] Error resuming animation:",o)}Ue.pausedAnimations=[],document.getElementById(sm)?.remove(),document.querySelectorAll("video").forEach(n=>{n.dataset.wasPaused==="false"&&(n.play().catch(()=>{}),delete n.dataset.wasPaused)})}function um(){let e=(0,Ac.useMemo)(()=>{let t=0,n=new Set,o=()=>(n.forEach(clearTimeout),n.clear(),++t),r=s=>s===t;return{start:o,isCurrent:r,schedule:(s,l,a)=>{if(!r(s))return;let c=De(()=>{n.delete(c),r(s)&&l()},a);n.add(c)}}},[]);return(0,Ac.useEffect)(()=>()=>{e.start()},[e]),e}function S0(e,t,n,o){let r=c=>o.get(c.id)??c.id,i=new Map(e.map(c=>[r(c),c])),s=new Map(t.map(c=>[r(c),c])),l=new Set(n.map(c=>c.id)),a=[];for(let c of n){let d=i.get(c.id),m=s.get(c.id);if(d&&!m)continue;let f=m&&d&&m.comment!==d.comment;a.push(f?{...c,comment:m.comment}:c)}for(let[c,d]of s)!i.has(c)&&!l.has(c)&&a.push(d);return a}function qi(e){if(e.tagName!=="IFRAME")return null;try{return e.contentDocument}catch{return null}}function yn(e,t=document){if(e===t)return null;try{return e.defaultView?.frameElement}catch{return null}}function kl(e){return e.nodeType===11&&"host"in e}function ri(e){let t=e.getBoundingClientRect(),n=e.offsetWidth?t.width/e.offsetWidth:1,o=e.offsetHeight?t.height/e.offsetHeight:1,r=e.ownerDocument.defaultView?.getComputedStyle(e),i=d=>parseFloat(d||"0")||0,s=i(r?.paddingLeft),l=i(r?.paddingTop),a=e.clientWidth-s-i(r?.paddingRight),c=e.clientHeight-l-i(r?.paddingBottom);return{x:t.left+(e.clientLeft+s)*n,y:t.top+(e.clientTop+l)*o,sx:n,sy:o,width:a*n,height:c*o}}function C0(e){try{let t=new URL(e);return t.origin+t.pathname}catch{return e}}function Gx(e,t,n,o=document){let r=yn(e,o);for(;r;){let i=ri(r);t=i.x+t*i.sx,n=i.y+n*i.sy,r=yn(r.ownerDocument,o)}return{x:t,y:n}}function $t(e,t=document){let n=e.getBoundingClientRect();return E0(e.ownerDocument,n,t)}function E0(e,t,n){if(!yn(e,n))return t;let o=t.left,r=t.top,i=t.right,s=t.bottom,l=yn(e,n);for(;l;){let a=ri(l);o=Math.max(a.x,a.x+o*a.sx),r=Math.max(a.y,a.y+r*a.sy),i=Math.min(a.x+a.width,a.x+i*a.sx),s=Math.min(a.y+a.height,a.y+s*a.sy),l=yn(l.ownerDocument,n)}return new DOMRect(o,r,Math.max(0,i-o),Math.max(0,s-r))}function Fc(e){let t=[];for(let n of e.querySelectorAll("*"))n.tagName==="IFRAME"&&t.push(n),n.shadowRoot&&n.tagName!=="AGENTATION-TOOLBAR"&&t.push(...Fc(n.shadowRoot));return t}function M0(e,t,n,o=document){let r=[];for(let m=yn(e.ownerDocument,o);m;m=yn(m.ownerDocument,o))r.unshift(m);if(!r.length)return;let i=r.map(m=>{let f=ri(m);return t=(t-f.x)/f.sx,n=(n-f.y)/f.sy,{index:Fc(m.ownerDocument).indexOf(m),id:m.id||void 0,url:qi(m)?.URL??""}}),s=e.ownerDocument.defaultView,l=!1;for(let m=e;m;m=m.parentElement)if(["fixed","sticky"].includes(s.getComputedStyle(m).position)){l=!0;break}let a=l?0:s.scrollX,c=l?0:s.scrollY,d=e.getBoundingClientRect();return{path:i,x:t+a,y:n+c,fixed:l,boundingBox:{x:d.left+a,y:d.top+c,width:d.width,height:d.height}}}function T0(e=document){let t=new Map,n=o=>{let r=t.get(o);return r||(r=Fc(o),t.set(o,r)),r};return o=>qx(o,e,n)}function qx(e,t=document,n=Fc){let o=e.frame;if(!o)return e;let r=t;for(let k of o.path){let w=n(r),C=k.id?w.find(g=>g.id===k.id):w[k.index],S=C&&qi(C);if(!S||C0(S.URL)!==C0(k.url))return null;r=S}let i=r.defaultView,s=o.fixed?0:i.scrollX,l=o.fixed?0:i.scrollY,a=o.x-s,c=o.y-l;for(let k=r,w=yn(r,t);w;w=yn(k,t)){let C=k.defaultView;if(a<0||c<0||a>C.innerWidth||c>C.innerHeight)return null;let S=ri(w);if(S.width<=0||S.height<=0)return null;a=S.x+a*S.sx,c=S.y+c*S.sy,k=w.ownerDocument}let d=o.boundingBox,m=E0(r,new DOMRect(d.x-s,d.y-l,d.width,d.height),t),f=t.defaultView;return{...e,x:a/f.innerWidth*100,y:c+(e.isFixed?0:f.scrollY),boundingBox:{x:m.x,y:m.y+(e.isFixed?0:f.scrollY),width:m.width,height:m.height}}}function Kx(e,t,n){if(!("clientX"in e)||t===n)return e;let o=Gx(t,e.clientX,e.clientY,n);return new Proxy(e,{get(r,i){if(i==="clientX")return o.x;if(i==="clientY")return o.y;let s=Reflect.get(r,i,r);return typeof s=="function"?s.bind(r):s}})}function L0(e,t){let n=new Set([e]),o=new Set,r=[],i=new Set,s=!1,l=!1,a,c=(m,f)=>{let k=w=>m.listener(Kx(w,f,e));m.handlers.set(f,k),f.addEventListener(m.type,k,m.options)},d=()=>{if(s=!1,!l)return;let m=new Set,f=new Set,k=[],w=C=>{k.push(C);for(let S of C.querySelectorAll("*"))if(!S.matches("agentation-toolbar, [data-agentation-portal]")&&(S.shadowRoot&&w(S.shadowRoot),S.tagName==="IFRAME")){f.add(S);let g=qi(S);g&&!m.has(g)&&(m.add(g),w(g))}};m.add(e),w(e);for(let C of n)if(!m.has(C)){for(let S of o){let g=S.handlers.get(C);g&&C.removeEventListener(S.type,g,S.options),S.handlers.delete(C)}n.delete(C)}for(let C of m)if(!n.has(C)){n.add(C);for(let S of o)c(S,C)}for(let C of i)f.has(C)||C.removeEventListener("load",d);for(let C of f)i.has(C)||C.addEventListener("load",d);i=f,a?.disconnect(),i.forEach(C=>a?.observe(C)),r.forEach(C=>C.disconnect()),r=k.map(C=>{let S=new MutationObserver(g=>{g.some(h=>[...h.addedNodes,...h.removedNodes].some(E=>E.nodeType===1&&!E.closest("agentation-toolbar, [data-agentation-portal]")&&(E.tagName==="IFRAME"||!!E.shadowRoot||!!E.querySelector("iframe"))))&&!s&&(s=!0,queueMicrotask(d))});return S.observe(C,{childList:!0,subtree:!0}),S}),t?.()};return{start(){l||(l=!0,typeof ResizeObserver=="function"&&(a=new ResizeObserver(()=>t?.())),d())},stop(){l=!1,a?.disconnect(),a=void 0,r.forEach(m=>m.disconnect()),r=[];for(let m of i)m.removeEventListener("load",d);i.clear();for(let m of o)for(let[f,k]of m.handlers)f.removeEventListener(m.type,k,m.options);o.clear(),n.clear(),n.add(e)},addEventListener(m,f,k){let w={type:m,listener:f,options:k,handlers:new Map};o.add(w);for(let C of n)c(w,C)},removeEventListener(m,f,k){for(let w of o)if(w.type===m&&w.listener===f){for(let[C,S]of w.handlers)C.removeEventListener(m,S,w.options);o.delete(w)}},querySelectorAll(m){return[...n].flatMap(f=>[...f.querySelectorAll(m)])}}}var Oc=["data-testid","data-test","data-qa","data-cy","data-component"];function Ki(e,t=Oc){let n={};for(let o of[...new Set(t)].slice(0,16)){if(!/^[a-zA-Z_][\w:.-]*$/.test(o))continue;let r=e.getAttribute(o);r!=null&&r.length<=500&&Object.defineProperty(n,o,{value:r,enumerable:!0})}return n}function R0(e,t=Oc){return Object.entries(Ki(e,t)).filter(([n,o])=>/^data-[a-z0-9_-]+$/.test(n)&&o.length<=120).slice(0,2).map(([n,o])=>`[${n}="${o.replace(/[\\"\n\r\f\0]/g,r=>r==="\\"||r==='"'?`\\${r}`:`\\${r.charCodeAt(0).toString(16)} `)}"]`).join("")}function Ji(e){if(e.parentElement)return e.parentElement;let t=e.getRootNode();return kl(t)?t.host:null}function jt(e,t){let n=e;for(;n;){if(n.matches(t))return n;n=Ji(n)}return null}function P0(e,t=4,n){let o=[],r=e,i=0;for(;r&&i<t;){let l=r.tagName.toLowerCase();if(l==="html"||l==="body"){o.length===0&&o.push(l);break}let a=l;if(r.id)a=`#${r.id}`;else if(r.className&&typeof r.className=="string"){let d=r.className.split(/\s+/).find(m=>m.length>2&&!m.match(/^[a-z]{1,2}$/)&&!m.match(/[A-Z0-9]{5,}/));d&&(a=`.${d.split("_")[0]}`)}a+=R0(r,n);let c=Ji(r);!r.parentElement&&c&&(a=`\u27E8shadow\u27E9 ${a}`),o.unshift(a),r=c,i++}let s=yn(e.ownerDocument);return(s?P0(s,2)+" > \u27E8iframe\u27E9 ":"")+o.join(" > ")}function Jx(e){let t="";for(let n of e.childNodes)if(n.nodeType===Node.TEXT_NODE){let o=n.textContent?.trim();o&&(t+=(t?" ":"")+o)}return t}function vr(e,t){let n=P0(e,4,t);if(e.dataset.element)return{name:e.dataset.element,path:n};let o=e.tagName.toLowerCase();if(["path","circle","rect","line","g"].includes(o)){let r=jt(e,"svg");if(r){let i=Ji(r);if(i?.namespaceURI==="http://www.w3.org/1999/xhtml")return{name:`graphic in ${vr(i).name}`,path:n}}return{name:"graphic element",path:n}}if(o==="svg"){let r=Ji(e);if(r?.tagName.toLowerCase()==="button"){let i=r.textContent?.trim();return{name:i?`icon in "${i}" button`:"button icon",path:n}}return{name:"icon",path:n}}if(o==="button"){let r=e.textContent?.trim(),i=e.getAttribute("aria-label");return i?{name:`button [${i}]`,path:n}:{name:r?`button "${r.slice(0,25)}"`:"button",path:n}}if(o==="a"){let r=e.textContent?.trim(),i=e.getAttribute("href");return r?{name:`link "${r.slice(0,25)}"`,path:n}:i?{name:`link to ${i.slice(0,30)}`,path:n}:{name:"link",path:n}}if(o==="input"){let r=e.getAttribute("type")||"text",i=e.getAttribute("placeholder"),s=e.getAttribute("name");return i?{name:`input "${i}"`,path:n}:s?{name:`input [${s}]`,path:n}:{name:`${r} input`,path:n}}if(["h1","h2","h3","h4","h5","h6"].includes(o)){let r=e.textContent?.trim();return{name:r?`${o} "${r.slice(0,35)}"`:o,path:n}}if(o==="p"){let r=e.textContent?.trim();return r?{name:`paragraph: "${r.slice(0,40)}${r.length>40?"...":""}"`,path:n}:{name:"paragraph",path:n}}if(o==="span"||o==="label"){let r=e.textContent?.trim();return r&&r.length<40?{name:`"${r}"`,path:n}:{name:o,path:n}}if(o==="li"){let r=e.textContent?.trim();return r&&r.length<40?{name:`list item: "${r.slice(0,35)}"`,path:n}:{name:"list item",path:n}}if(o==="blockquote")return{name:"blockquote",path:n};if(o==="code"){let r=e.textContent?.trim();return r&&r.length<30?{name:`code: \`${r}\``,path:n}:{name:"code",path:n}}if(o==="pre")return{name:"code block",path:n};if(o==="img"){let r=e.getAttribute("alt");return{name:r?`image "${r.slice(0,30)}"`:"image",path:n}}if(o==="video")return{name:"video",path:n};if(["div","section","article","nav","header","footer","aside","main"].includes(o)){let r=e.className,i=e.getAttribute("role"),s=e.getAttribute("aria-label");if(s)return{name:`${o} [${s}]`,path:n};if(i)return{name:`${i}`,path:n};let l=Jx(e);if(l&&l.length<50)return{name:`"${l}"`,path:n};if(typeof r=="string"&&r){let a=r.split(/[\s_-]+/).map(c=>c.replace(/[A-Z0-9]{5,}.*$/,"")).filter(c=>c.length>2&&!/^[a-z]{1,2}$/.test(c)).slice(0,2);if(a.length>0)return{name:a.join(" "),path:n}}return{name:o==="div"?"container":o,path:n}}return{name:o,path:n}}function Zi(e){let t=[],n=e.textContent?.trim();n&&n.length<100&&t.push(n);let o=e.previousElementSibling;if(o){let i=o.textContent?.trim();i&&i.length<50&&t.unshift(`[before: "${i.slice(0,40)}"]`)}let r=e.nextElementSibling;if(r){let i=r.textContent?.trim();i&&i.length<50&&t.push(`[after: "${i.slice(0,40)}"]`)}return t.join(" ")}function Sl(e){let t=Ji(e);if(!t)return"";let n=e.getRootNode(),r=(kl(n)&&e.parentElement?Array.from(e.parentElement.children):Array.from(t.children)).filter(d=>d!==e&&d.namespaceURI==="http://www.w3.org/1999/xhtml");if(r.length===0)return"";let i=r.slice(0,4).map(d=>{let m=d.tagName.toLowerCase(),f=d.className,k="";if(typeof f=="string"&&f){let w=f.split(/\s+/).map(C=>C.replace(/[_][a-zA-Z0-9]{5,}.*$/,"")).find(C=>C.length>2&&!/^[a-z]{1,2}$/.test(C));w&&(k=`.${w}`)}if(m==="button"||m==="a"){let w=d.textContent?.trim().slice(0,15);if(w)return`${m}${k} "${w}"`}return`${m}${k}`}),l=t.tagName.toLowerCase();if(typeof t.className=="string"&&t.className){let d=t.className.split(/\s+/).map(m=>m.replace(/[_][a-zA-Z0-9]{5,}.*$/,"")).find(m=>m.length>2&&!/^[a-z]{1,2}$/.test(m));d&&(l=`.${d}`)}let a=t.children.length,c=a>i.length+1?` (${a} total in ${l})`:"";return i.join(", ")+c}function es(e){let t=e.className;return typeof t!="string"||!t?"":t.split(/\s+/).filter(o=>o.length>0).map(o=>{let r=o.match(/^([a-zA-Z][a-zA-Z0-9_-]*?)(?:_[a-zA-Z0-9]{5,})?$/);return r?r[1]:o}).filter((o,r,i)=>i.indexOf(o)===r).join(", ")}var N0=new Set(["none","normal","auto","0px","rgba(0, 0, 0, 0)","transparent","static","visible"]),Zx=new Set(["p","span","h1","h2","h3","h4","h5","h6","label","li","td","th","blockquote","figcaption","caption","legend","dt","dd","pre","code","em","strong","b","i","a","time","cite","q"]),ev=new Set(["input","textarea","select"]),tv=new Set(["img","video","canvas","svg"]),nv=new Set(["div","section","article","nav","header","footer","aside","main","ul","ol","form","fieldset"]);function Cl(e){if(typeof window>"u")return{};let t=(e.ownerDocument.defaultView??window).getComputedStyle(e),n={},o=e.tagName.toLowerCase(),r;Zx.has(o)?r=["color","fontSize","fontWeight","fontFamily","lineHeight"]:o==="button"||o==="a"&&e.getAttribute("role")==="button"?r=["backgroundColor","color","padding","borderRadius","fontSize"]:ev.has(o)?r=["backgroundColor","color","padding","borderRadius","fontSize"]:tv.has(o)?r=["width","height","objectFit","borderRadius"]:nv.has(o)?r=["display","padding","margin","gap","backgroundColor"]:r=["color","fontSize","margin","padding","backgroundColor"];for(let i of r){let s=i.replace(/([A-Z])/g,"-$1").toLowerCase(),l=t.getPropertyValue(s);l&&!N0.has(l)&&(n[i]=l)}return n}var ov=["color","backgroundColor","borderColor","fontSize","fontWeight","fontFamily","lineHeight","letterSpacing","textAlign","width","height","padding","margin","border","borderRadius","display","position","top","right","bottom","left","zIndex","flexDirection","justifyContent","alignItems","gap","opacity","visibility","overflow","boxShadow","transform"];function El(e){if(typeof window>"u")return"";let t=(e.ownerDocument.defaultView??window).getComputedStyle(e),n=[];for(let o of ov){let r=o.replace(/([A-Z])/g,"-$1").toLowerCase(),i=t.getPropertyValue(r);i&&!N0.has(i)&&n.push(`${r}: ${i}`)}return n.join("; ")}function I0(e){if(!e)return;let t={},n=e.split(";").map(o=>o.trim()).filter(Boolean);for(let o of n){let r=o.indexOf(":");if(r>0){let i=o.slice(0,r).trim(),s=o.slice(r+1).trim();i&&s&&(t[i]=s)}}return Object.keys(t).length>0?t:void 0}function Ml(e){let t=[],n=e.getAttribute("role"),o=e.getAttribute("aria-label"),r=e.getAttribute("aria-describedby"),i=e.getAttribute("tabindex"),s=e.getAttribute("aria-hidden");return n&&t.push(`role="${n}"`),o&&t.push(`aria-label="${o}"`),r&&t.push(`aria-describedby="${r}"`),i&&t.push(`tabindex=${i}`),s==="true"&&t.push("aria-hidden"),e.matches("a, button, input, select, textarea, [tabindex]")&&t.push("focusable"),t.join(", ")}function ts(e){let t=[],n=e;for(;n&&n.tagName.toLowerCase()!=="html";){let r=n.tagName.toLowerCase(),i=r;if(n.id)i=`${r}#${n.id}`;else if(n.className&&typeof n.className=="string"){let l=n.className.split(/\s+/).map(a=>a.replace(/[_][a-zA-Z0-9]{5,}.*$/,"")).find(a=>a.length>2);l&&(i=`${r}.${l}`)}let s=Ji(n);!n.parentElement&&s&&(i=`\u27E8shadow\u27E9 ${i}`),t.unshift(i),n=s}let o=yn(e.ownerDocument);return(o?ts(o)+" > \u27E8iframe\u27E9 ":"")+t.join(" > ")}var D0="agentation-toolbar, [data-agentation-root], [data-feedback-toolbar], [data-annotation-popup], [data-annotation-marker]",rv=new Set(["DIV","SPAN","SECTION","ARTICLE","MAIN","ASIDE","HEADER","FOOTER","NAV"]);function Tl(e,t){let n=document.elementFromPoint(e,t),o=new Set;for(;n&&!o.has(n);){o.add(n);let r=qi(n),i;if(r){let s=ri(n);e=(e-s.x)/s.sx,t=(t-s.y)/s.sy,i=r.elementFromPoint?.(e,t)}else i=n.shadowRoot?.elementFromPoint?.(e,t);if(!i||i===n)break;n=i}return n}function iv(e){if(typeof e.checkVisibility=="function")return e.checkVisibility({checkOpacity:!0,checkVisibilityCSS:!0});let t=getComputedStyle(e);if(t.visibility==="hidden"||t.visibility==="collapse")return!1;let n=e;for(;n;){let o=getComputedStyle(n);if(o.opacity==="0"||o.display==="none"||o.contentVisibility==="hidden")return!1;let r=n.getRootNode();n=n.parentElement||(kl(r)?r.host:null)}return!0}function $0(e,t){let n=[],o=new Set,r=(i,s,l)=>{for(let a of i){if(o.has(a))continue;if(o.add(a),a.shadowRoot){let d=a.shadowRoot,m=d.elementsFromPoint?.(s,l)??[];r(m.length?m:[d.elementFromPoint?.(s,l)].filter(Boolean),s,l)}let c=qi(a);if(c){let d=ri(a),m=(s-d.x)/d.sx,f=(l-d.y)/d.sy;r(c.elementsFromPoint?.(m,f)??[c.elementFromPoint?.(m,f)].filter(Boolean),m,f)}a!==a.ownerDocument.body&&a!==a.ownerDocument.documentElement&&!jt(a,D0)&&iv(a)&&n.push(a)}};return r(document.elementsFromPoint?.(e,t)??[document.elementFromPoint(e,t)].filter(Boolean),e,t),n}function Ll(e,t,n){if(n.width<=0||n.height<=0)return null;let o=null,r=1/0;for(let i of $0(e,t)){let s=$t(i),l=s.width/n.width,a=s.height/n.height;if(l<.5||l>2||a<.5||a>2)continue;let c=Math.abs(Math.log(l))+Math.abs(Math.log(a));c<r&&(o=i,r=c)}return o}function dm(e,t){let n=Tl(e,t);if(!n||jt(n,D0))return null;let o=$0(e,t);for(let s of o)if(!rv.has(s.tagName)&&!s.shadowRoot||Array.from(s.childNodes).some(l=>l.nodeType===Node.TEXT_NODE&&l.textContent?.trim()))return s;let r=null,i=1/0;for(let s of o){let l=$t(s),a=l.width*l.height;a>0&&a<i&&(r=s,i=a)}return r}var T=ce(vt());var zc=ce(vt());function A0(e){let t=(0,zc.useCallback)(n=>e?(window.addEventListener("hashchange",n),window.addEventListener("popstate",n),()=>{window.removeEventListener("hashchange",n),window.removeEventListener("popstate",n)}):()=>{},[e]);return(0,zc.useSyncExternalStore)(t,()=>window.location.pathname+(e?window.location.hash:""),()=>"/")}var Bc=new Map;function F0(e,t){let n=(Bc.get(e)??Promise.resolve()).then(t),o=n.then(()=>{},()=>{});return Bc.set(e,o),o.then(()=>{Bc.get(e)===o&&Bc.delete(e)}),n}function O0(e,t,n){try{let o=new URL(e,n);return o.origin===n&&o.pathname+o.hash===t}catch{return!1}}var X1=ce(bl());var ns=ce(vt()),B0=typeof window>"u"?ns.useEffect:ns.useLayoutEffect;function z0(e){let[t,n]=(0,ns.useState)(null);return B0(()=>{let o=document.createElement("div");return o.setAttribute("data-agentation-portal",""),o.style.display="contents",n(o),()=>o.remove()},[]),B0(()=>{if(!t)return;let o=e??document.body;if(o.ownerDocument!==document){console.warn("[Agentation] portalContainer belongs to another document; the toolbar will not render.");return}let r=document.activeElement;for(;r?.shadowRoot?.activeElement;)r=r.shadowRoot.activeElement;let i=r&&t.contains(document.activeElement)?r:null;typeof t.hidePopover=="function"&&t.matches(":popover-open")&&t.hidePopover(),o.appendChild(t),e&&typeof t.showPopover=="function"?(t.setAttribute("popover","manual"),t.style.cssText="position:fixed;inset:0 auto auto 0;margin:0;padding:0;border:0;background:transparent;width:0;height:0;overflow:visible;pointer-events:none",t.showPopover()):(t.removeAttribute("popover"),t.style.cssText="display:contents"),i?.focus({preventScroll:!0})},[t,e]),t}var os=ce(vt()),j0=ce(bl()),V0=ce(ze()),Y0=({mode:e="open",delegatesFocus:t,slotAssignment:n,host:o="div",children:r,className:i,...s})=>{let l=(0,os.useRef)(null),[a,c]=(0,os.useState)(null);return(0,os.useLayoutEffect)(()=>{let m=l.current;if(!m||m.shadowRoot)return;let f=m.attachShadow({mode:e,delegatesFocus:t,slotAssignment:n});c(f)},[]),(0,V0.jsx)(o,{ref:l,...s,...o.includes("-")?{class:i}:{className:i},children:a&&(0,j0.createPortal)(r,a)})};var Qt=ce(vt());var Rl=ce(vt());function rs(e,t,n){let o=(0,Rl.useRef)(n);(0,Rl.useLayoutEffect)(()=>{o.current=n},[n]),(0,Rl.useLayoutEffect)(()=>{let r=e.current;if(!t||!r)return;let i=!1,s=r.getAnimations?.()??[];return Promise.allSettled(s.map(l=>l.finished)).then(()=>{i||o.current()}),()=>{i=!0}},[e,t])}var Wc=`@charset "UTF-8";
.styles-module__popup___IhzrD svg[fill=none] {
  fill: none !important;
}
.styles-module__popup___IhzrD svg[fill=none] :not([fill]) {
  fill: none !important;
}

@keyframes styles-module__popupEnter___AuQDN {
  from {
    opacity: 0;
    transform: translateX(-50%) scale(0.95) translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateX(-50%) scale(1) translateY(0);
  }
}
@keyframes styles-module__popupExit___JJKQX {
  from {
    opacity: 1;
    transform: translateX(-50%) scale(1) translateY(0);
  }
  to {
    opacity: 0;
    transform: translateX(-50%) scale(0.95) translateY(4px);
  }
}
@keyframes styles-module__shake___jdbWe {
  0%, 100% {
    transform: translateX(-50%) scale(1) translateY(0) translateX(0);
  }
  20% {
    transform: translateX(-50%) scale(1) translateY(0) translateX(-3px);
  }
  40% {
    transform: translateX(-50%) scale(1) translateY(0) translateX(3px);
  }
  60% {
    transform: translateX(-50%) scale(1) translateY(0) translateX(-2px);
  }
  80% {
    transform: translateX(-50%) scale(1) translateY(0) translateX(2px);
  }
}
.styles-module__popup___IhzrD {
  position: fixed;
  transform: translateX(-50%);
  width: 280px;
  padding: 0.75rem 1rem;
  background: #1a1a1a;
  border-radius: 16px;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.3), 0 0 0 1px rgba(255, 255, 255, 0.08);
  z-index: 100001;
  font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  will-change: transform, opacity;
  opacity: 0;
}
.styles-module__popup___IhzrD.styles-module__enter___L7U7N {
  animation: styles-module__popupEnter___AuQDN 0.2s cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
}
.styles-module__popup___IhzrD.styles-module__entered___COX-w {
  opacity: 1;
  transform: translateX(-50%) scale(1) translateY(0);
}
.styles-module__popup___IhzrD.styles-module__exit___5eGjE {
  pointer-events: none;
  animation: styles-module__popupExit___JJKQX 0.15s ease-in forwards;
}
.styles-module__popup___IhzrD.styles-module__entered___COX-w.styles-module__shake___jdbWe {
  animation: styles-module__shake___jdbWe 0.25s ease-out;
}

.styles-module__header___wWsSi {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.5625rem;
}

.styles-module__element___fTV2z {
  font-size: 0.75rem;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.5);
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.styles-module__headerToggle___WpW0b {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  flex: 1;
  min-width: 0;
  text-align: left;
}
.styles-module__headerToggle___WpW0b .styles-module__element___fTV2z {
  flex: 1;
}

.styles-module__chevron___ZZJlR {
  color: rgba(255, 255, 255, 0.5);
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  flex-shrink: 0;
}
.styles-module__chevron___ZZJlR.styles-module__expanded___2Hxgv {
  transform: rotate(90deg);
}

.styles-module__stylesWrapper___pnHgy {
  display: grid;
  grid-template-rows: 0fr;
  transition: grid-template-rows 0.3s cubic-bezier(0.16, 1, 0.3, 1);
}
.styles-module__stylesWrapper___pnHgy.styles-module__expanded___2Hxgv {
  grid-template-rows: 1fr;
}

.styles-module__stylesInner___YYZe2 {
  overflow: hidden;
}

.styles-module__stylesBlock___VfQKn {
  background: rgba(255, 255, 255, 0.05);
  border-radius: 0.375rem;
  padding: 0.5rem 0.625rem;
  margin-bottom: 0.5rem;
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.6875rem;
  line-height: 1.5;
}

.styles-module__styleLine___1YQiD {
  color: rgba(255, 255, 255, 0.85);
  word-break: break-word;
}

.styles-module__styleProperty___84L1i {
  color: #c792ea;
}

.styles-module__styleValue___q51-h {
  color: rgba(255, 255, 255, 0.85);
}

.styles-module__timestamp___Dtpsv {
  font-size: 0.625rem;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.35);
  font-variant-numeric: tabular-nums;
  margin-left: 0.5rem;
  flex-shrink: 0;
}

.styles-module__quote___mcMmQ {
  font-size: 12px;
  font-style: italic;
  color: rgba(255, 255, 255, 0.6);
  margin-bottom: 0.5rem;
  padding: 0.4rem 0.5rem;
  background: rgba(255, 255, 255, 0.05);
  border-radius: 0.25rem;
  line-height: 1.45;
}

.styles-module__screenshot___7dYA7 {
  display: block;
  width: auto;
  height: auto;
  max-width: 100%;
  max-height: 150px;
  margin: 0 auto 0.5rem;
  border-radius: 0.25rem;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.styles-module__textarea___jrSae {
  box-sizing: border-box;
  width: 100%;
  padding: 0.5rem 0.625rem;
  font-size: 0.8125rem;
  font-family: inherit;
  background: rgba(255, 255, 255, 0.05);
  color: #fff;
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 8px;
  resize: none;
  outline: none;
  transition: border-color 0.15s ease;
}
.styles-module__textarea___jrSae:focus {
  border-color: var(--agentation-color-blue);
}
.styles-module__textarea___jrSae.styles-module__green___99l3h:focus {
  border-color: var(--agentation-color-green);
}
.styles-module__textarea___jrSae::placeholder {
  color: rgba(255, 255, 255, 0.35);
}
.styles-module__textarea___jrSae::-webkit-scrollbar {
  width: 6px;
}
.styles-module__textarea___jrSae::-webkit-scrollbar-track {
  background: transparent;
}
.styles-module__textarea___jrSae::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.2);
  border-radius: 3px;
}

.styles-module__actions___D6x3f {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 0.375rem;
  margin-top: 0.75rem;
}

.styles-module__sourceAction___EabJb {
  display: block;
  max-width: 100%;
  margin: -2px 0 8px;
  padding: 2px 0;
  background: transparent;
  color: #fff;
  font: inherit;
  font-size: 11px;
  border: 0;
  cursor: pointer;
  opacity: 0.6;
  transition: opacity 0.15s ease;
}
.styles-module__sourceAction___EabJb:hover, .styles-module__sourceAction___EabJb:focus-visible {
  opacity: 1;
}
.styles-module__sourceAction___EabJb:focus-visible {
  outline: 2px solid currentColor;
  outline-offset: 3px;
}

.styles-module__light___6AaSQ .styles-module__sourceAction___EabJb {
  color: #111;
}

.styles-module__cancel___hRjnL,
.styles-module__submit___K-mIR,
.styles-module__deleteButton___4VuAE {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-height: 1.875rem;
  padding: 0.375rem 0.875rem;
  font-size: 0.75rem;
  font-weight: 500;
  border-radius: 1rem;
  border: none;
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease, opacity 0.15s ease;
}

.styles-module__cancel___hRjnL {
  background: transparent;
  color: rgba(255, 255, 255, 0.5);
}
.styles-module__cancel___hRjnL:hover {
  background: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.8);
}

.styles-module__submit___K-mIR {
  color: white;
}
.styles-module__submit___K-mIR:hover:not(:disabled) {
  filter: brightness(0.9);
}
.styles-module__submit___K-mIR:disabled {
  cursor: not-allowed;
}

.styles-module__deleteWrapper___oSjdo {
  display: flex;
  margin-right: auto;
}

.styles-module__deleteButton___4VuAE {
  background: transparent;
  color: rgba(255, 255, 255, 0.4);
  transition: background-color 0.15s ease, color 0.15s ease, transform 0.1s ease;
}
.styles-module__deleteButton___4VuAE:hover {
  background-color: color-mix(in srgb, var(--agentation-color-red) 25%, transparent);
  color: var(--agentation-color-red);
}
.styles-module__deleteButton___4VuAE:active {
  transform: scale(0.92);
}

.styles-module__light___6AaSQ.styles-module__popup___IhzrD {
  background: #fff;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.12), 0 0 0 1px rgba(0, 0, 0, 0.06);
}
.styles-module__light___6AaSQ .styles-module__element___fTV2z {
  color: rgba(0, 0, 0, 0.6);
}
.styles-module__light___6AaSQ .styles-module__timestamp___Dtpsv {
  color: rgba(0, 0, 0, 0.4);
}
.styles-module__light___6AaSQ .styles-module__chevron___ZZJlR {
  color: rgba(0, 0, 0, 0.4);
}
.styles-module__light___6AaSQ .styles-module__stylesBlock___VfQKn {
  background: rgba(0, 0, 0, 0.03);
}
.styles-module__light___6AaSQ .styles-module__styleLine___1YQiD {
  color: rgba(0, 0, 0, 0.75);
}
.styles-module__light___6AaSQ .styles-module__styleProperty___84L1i {
  color: #7c3aed;
}
.styles-module__light___6AaSQ .styles-module__styleValue___q51-h {
  color: rgba(0, 0, 0, 0.75);
}
.styles-module__light___6AaSQ .styles-module__quote___mcMmQ {
  color: rgba(0, 0, 0, 0.55);
  background: rgba(0, 0, 0, 0.04);
}
.styles-module__light___6AaSQ .styles-module__textarea___jrSae {
  background: rgba(0, 0, 0, 0.03);
  color: #1a1a1a;
  border-color: rgba(0, 0, 0, 0.12);
}
.styles-module__light___6AaSQ .styles-module__textarea___jrSae::placeholder {
  color: rgba(0, 0, 0, 0.4);
}
.styles-module__light___6AaSQ .styles-module__textarea___jrSae::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.15);
}
.styles-module__light___6AaSQ .styles-module__cancel___hRjnL {
  color: rgba(0, 0, 0, 0.5);
}
.styles-module__light___6AaSQ .styles-module__cancel___hRjnL:hover {
  background: rgba(0, 0, 0, 0.06);
  color: rgba(0, 0, 0, 0.75);
}
.styles-module__light___6AaSQ .styles-module__deleteButton___4VuAE {
  color: rgba(0, 0, 0, 0.4);
}
.styles-module__light___6AaSQ .styles-module__deleteButton___4VuAE:hover {
  background-color: color-mix(in srgb, var(--agentation-color-red) 25%, transparent);
  color: var(--agentation-color-red);
}

@media (prefers-reduced-motion: reduce) {
  .styles-module__popup___IhzrD.styles-module__enter___L7U7N, .styles-module__popup___IhzrD.styles-module__exit___5eGjE {
    animation-duration: 1ms;
    animation-delay: 0ms !important;
  }
}
.styles-module__sharedForm___8GvQl {
  --card-motion: 200ms cubic-bezier(0.2, 0.8, 0.2, 1);
  padding: 0.75rem 1rem;
  transition: padding var(--card-motion);
}
.styles-module__sharedForm___8GvQl .styles-module__header___wWsSi {
  transition: margin-bottom var(--card-motion);
}
.styles-module__sharedForm___8GvQl .styles-module__element___fTV2z {
  font-style: italic;
  color: rgba(255, 255, 255, 0.6);
}
.styles-module__sharedForm___8GvQl .styles-module__previewExcerpt___DCOIL {
  display: none;
}
.styles-module__sharedForm___8GvQl .styles-module__headerToggle___WpW0b .styles-module__chevron___ZZJlR {
  opacity: 1;
  margin-left: 0;
  transition: margin-left var(--card-motion), opacity 100ms ease-out, transform var(--card-motion);
}
.styles-module__sharedForm___8GvQl .styles-module__sharedNote___OYVi5 {
  position: relative;
  height: var(--editor-field-height, 57px);
  border-radius: 8px;
  overflow: clip;
  transition: height var(--card-motion);
}
.styles-module__sharedForm___8GvQl .styles-module__sharedNote___OYVi5::before {
  content: "";
  position: absolute;
  inset: 0;
  border: 1px solid var(--field-border, rgba(255, 255, 255, 0.15));
  border-radius: inherit;
  background: rgba(255, 255, 255, 0.05);
  pointer-events: none;
  transition: opacity var(--card-motion), border-color var(--card-motion);
}
.styles-module__sharedForm___8GvQl .styles-module__sharedNoteContent___6q3KD {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: clip;
  transition: width var(--card-motion);
}
.styles-module__sharedForm___8GvQl .styles-module__sharedNote___OYVi5[data-truncated]::after {
  content: "\u2026";
  position: absolute;
  right: 0;
  top: 0;
  color: #fff;
  font-size: 13px;
  line-height: 1.4;
  opacity: 0;
  pointer-events: none;
  transition: opacity 80ms ease-out;
}
.styles-module__sharedForm___8GvQl .styles-module__textarea___jrSae {
  display: block;
  width: calc(280px - 2rem);
  max-width: calc(100vw - 24px - 2rem);
  margin: 0;
  background: transparent !important;
  border-color: transparent !important;
  transform: translate(0, 0);
  transition: transform var(--card-motion), color var(--card-motion);
}
.styles-module__sharedForm___8GvQl .styles-module__sharedExtra___RUBKC, .styles-module__sharedForm___8GvQl .styles-module__sharedActions___6Glpl {
  display: grid;
  grid-template-rows: 1fr;
  opacity: 1;
  transition: grid-template-rows var(--card-motion), opacity 80ms ease-out;
}
.styles-module__sharedForm___8GvQl .styles-module__sharedActions___6Glpl {
  transition-delay: 0ms, 120ms;
}
.styles-module__sharedForm___8GvQl .styles-module__sharedExtraInner___EuUh4 {
  min-height: 0;
  overflow: hidden;
}
.styles-module__sharedForm___8GvQl .styles-module__actions___D6x3f {
  min-height: 0;
  overflow: hidden;
  transition: margin-top var(--card-motion);
}
.styles-module__sharedForm___8GvQl[data-preview] {
  padding: 8px 12px;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__header___wWsSi {
  margin-bottom: 5px;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__previewExcerpt___DCOIL {
  display: inline;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__element___fTV2z {
  line-height: 1.4;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__chevron___ZZJlR {
  opacity: 0;
  margin-left: -18px;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__sharedNote___OYVi5 {
  height: 20.2px;
  border-radius: 0;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__sharedNote___OYVi5::before {
  opacity: 0;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__textarea___jrSae {
  transform: translate(-11px, calc(-9px + (1.4em - 1lh) / 2));
  color: #fff;
  overflow: hidden;
  cursor: default;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__sharedExtra___RUBKC, .styles-module__sharedForm___8GvQl[data-preview] .styles-module__sharedActions___6Glpl {
  grid-template-rows: 0fr;
  opacity: 0;
  transition-delay: 0ms;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__actions___D6x3f {
  margin-top: 0;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__stylesWrapper___pnHgy {
  grid-template-rows: 0fr;
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__sharedNote___OYVi5[data-truncated] .styles-module__sharedNoteContent___6q3KD {
  width: calc(100% - 12px);
}
.styles-module__sharedForm___8GvQl[data-preview] .styles-module__sharedNote___OYVi5[data-truncated]::after {
  opacity: 1;
}

.styles-module__light___6AaSQ .styles-module__sharedForm___8GvQl .styles-module__sharedNote___OYVi5::before {
  border-color: var(--field-border, rgba(0, 0, 0, 0.12));
  background: rgba(0, 0, 0, 0.03);
}

.styles-module__light___6AaSQ .styles-module__sharedForm___8GvQl .styles-module__element___fTV2z {
  color: rgba(0, 0, 0, 0.5);
}

.styles-module__light___6AaSQ .styles-module__sharedForm___8GvQl[data-preview] .styles-module__textarea___jrSae, .styles-module__light___6AaSQ .styles-module__sharedForm___8GvQl[data-preview] .styles-module__sharedNote___OYVi5::after {
  color: rgba(0, 0, 0, 0.85);
}

@media (prefers-reduced-motion: reduce) {
  .styles-module__sharedForm___8GvQl {
    --card-motion: 1ms linear;
  }
  .styles-module__sharedForm___8GvQl .styles-module__sharedActions___6Glpl, .styles-module__sharedForm___8GvQl .styles-module__headerToggle___WpW0b .styles-module__chevron___ZZJlR {
    transition: opacity 100ms ease-out;
    transition-delay: 0ms;
  }
}`,Re={popup:"styles-module__popup___IhzrD",enter:"styles-module__enter___L7U7N",popupEnter:"styles-module__popupEnter___AuQDN",entered:"styles-module__entered___COX-w",exit:"styles-module__exit___5eGjE",popupExit:"styles-module__popupExit___JJKQX",shake:"styles-module__shake___jdbWe",header:"styles-module__header___wWsSi",element:"styles-module__element___fTV2z",headerToggle:"styles-module__headerToggle___WpW0b",chevron:"styles-module__chevron___ZZJlR",expanded:"styles-module__expanded___2Hxgv",stylesWrapper:"styles-module__stylesWrapper___pnHgy",stylesInner:"styles-module__stylesInner___YYZe2",stylesBlock:"styles-module__stylesBlock___VfQKn",styleLine:"styles-module__styleLine___1YQiD",styleProperty:"styles-module__styleProperty___84L1i",styleValue:"styles-module__styleValue___q51-h",timestamp:"styles-module__timestamp___Dtpsv",quote:"styles-module__quote___mcMmQ",screenshot:"styles-module__screenshot___7dYA7",textarea:"styles-module__textarea___jrSae",green:"styles-module__green___99l3h",actions:"styles-module__actions___D6x3f",sourceAction:"styles-module__sourceAction___EabJb",light:"styles-module__light___6AaSQ",cancel:"styles-module__cancel___hRjnL",submit:"styles-module__submit___K-mIR",deleteButton:"styles-module__deleteButton___4VuAE",deleteWrapper:"styles-module__deleteWrapper___oSjdo",sharedForm:"styles-module__sharedForm___8GvQl",previewExcerpt:"styles-module__previewExcerpt___DCOIL",sharedNote:"styles-module__sharedNote___OYVi5",sharedNoteContent:"styles-module__sharedNoteContent___6q3KD",sharedExtra:"styles-module__sharedExtra___RUBKC",sharedActions:"styles-module__sharedActions___6Glpl",sharedExtraInner:"styles-module__sharedExtraInner___EuUh4"};var X0=ce(vt()),_m="data-agentation-styles";function mm(e,t,n){if(!n||!e)return;let o=e.nodeType===9?e.head:e.nodeType===11?e:null;if(!o||typeof o.querySelector!="function"||o.querySelector(`style[${_m}~="toolbar"], style[${_m}~="${t}"]`))return;let i=(e.nodeType===9?e:e.ownerDocument).createElement("style");i.setAttribute(_m,t),i.textContent=n,o.appendChild(i)}function Uc(e,t){return(0,X0.useCallback)(n=>{n&&mm(n.getRootNode(),e,t)},[e,t])}var dn=ce(vt());var Xe=ce(ze());function Q0(e){if(!e)return;let t=n=>n.stopImmediatePropagation();document.addEventListener("focusin",t,!0),document.addEventListener("focusout",t,!0);try{e.focus({preventScroll:!0})}finally{document.removeEventListener("focusin",t,!0),document.removeEventListener("focusout",t,!0)}}var jc=(0,dn.forwardRef)(function({element:t,timestamp:n,selectedText:o,screenshot:r,placeholder:i="What should change?",initialValue:s="",submitLabel:l="Add",onSubmit:a,onCancel:c,onDelete:d,onOpenSource:m,allowEmpty:f=!1,accentColor:k="#3c82f7",computedStyles:w,disabled:C=!1,preview:S=!1,resetOnPreview:g=!0,variant:x="popup"},h){let E=x==="card",[F,W]=(0,dn.useState)(s),[D,j]=(0,dn.useState)(!1),[Z,q]=(0,dn.useState)(!1),O=(0,dn.useRef)(null),Ae=(0,dn.useRef)(null),ue=o?` "${o.slice(0,30)}${o.length>30?"...":""}"`:"";(0,dn.useLayoutEffect)(()=>{let K=Ae.current,Ge=O.current;if(!E||!K||!Ge)return;let Le=()=>{K.style.setProperty("--editor-field-height",`${Ge.offsetHeight}px`)};if(Le(),"CanvasRenderingContext2D"in window){let xe=document.createElement("canvas").getContext("2d");if(xe){let gt=getComputedStyle(Ge).fontFamily;xe.font=`13px ${gt}`;let Ft=xe.measureText(s.replace(/\s+/g," ")).width;xe.font=`italic 12px ${gt}`;let Kn=xe.measureText(t+ue).width,_n=Math.min(200,Math.max(120,Math.ceil(Math.max(Ft,Kn))+24));K.closest("[data-annotation-card]")?.style.setProperty("--preview-width",`${_n}px`);let Jn=K.querySelector("[data-shared-note]");Jn&&Jn.toggleAttribute("data-truncated",Ft>_n-24)}}let Ye=typeof ResizeObserver<"u"?new ResizeObserver(Le):null;return Ye?.observe(Ge),()=>Ye?.disconnect()},[E,t,s,ue]),(0,dn.useLayoutEffect)(()=>{S&&g&&(W(s),q(!1),O.current&&(O.current.scrollTop=0,O.current.scrollLeft=0))},[S,g,s]),(0,dn.useImperativeHandle)(h,()=>({focus(){let K=O.current;Q0(K),K&&(K.selectionStart=K.selectionEnd=K.value.length,K.scrollTop=E?0:K.scrollHeight)}}),[E]);let J=(0,dn.useCallback)(()=>{C||!F.trim()&&!f||a(F.trim())},[C,F,f,a]),Ce=K=>{K.stopPropagation(),!K.nativeEvent.isComposing&&(K.key==="Enter"&&!K.shiftKey&&(K.preventDefault(),J()),K.key==="Escape"&&c())};return(0,Xe.jsxs)("div",{ref:Ae,className:E?Re.sharedForm:void 0,style:E?void 0:{display:"contents"},"data-annotation-editor":!0,"data-preview":S||void 0,children:[(0,Xe.jsxs)("div",{className:Re.header,"data-editor-heading":!0,children:[w&&Object.keys(w).length>0?(0,Xe.jsxs)("button",{className:Re.headerToggle,onClick:()=>{let K=Z;q(!Z),K&&De(()=>Q0(O.current),0)},type:"button",children:[(0,Xe.jsx)("svg",{className:`${Re.chevron} ${Z?Re.expanded:""}`,width:"14",height:"14",viewBox:"0 0 14 14",fill:"none",xmlns:"http://www.w3.org/2000/svg",children:(0,Xe.jsx)("path",{d:"M5.5 10.25L9 7.25L5.75 4",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})}),(0,Xe.jsxs)("span",{className:Re.element,children:[t,E&&ue&&(0,Xe.jsx)("span",{className:Re.previewExcerpt,children:ue})]})]}):(0,Xe.jsxs)("span",{className:Re.element,children:[t,E&&ue&&(0,Xe.jsx)("span",{className:Re.previewExcerpt,children:ue})]}),n&&(0,Xe.jsx)("span",{className:Re.timestamp,children:n})]}),m&&(0,Xe.jsx)("div",{className:E?Re.sharedExtra:void 0,style:E?void 0:{display:"contents"},children:(0,Xe.jsx)("div",{className:E?Re.sharedExtraInner:void 0,style:E?void 0:{display:"contents"},children:(0,Xe.jsx)("button",{type:"button",className:Re.sourceAction,onClick:m,children:"Open in editor"})})}),w&&Object.keys(w).length>0&&(0,Xe.jsx)("div",{className:`${Re.stylesWrapper} ${Z?Re.expanded:""}`,children:(0,Xe.jsx)("div",{className:Re.stylesInner,children:(0,Xe.jsx)("div",{className:Re.stylesBlock,children:Object.entries(w).map(([K,Ge])=>(0,Xe.jsxs)("div",{className:Re.styleLine,children:[(0,Xe.jsx)("span",{className:Re.styleProperty,children:K.replace(/([A-Z])/g,"-$1").toLowerCase()}),": ",(0,Xe.jsx)("span",{className:Re.styleValue,children:Ge}),";"]},K))})})}),o&&(0,Xe.jsx)("div",{className:E?Re.sharedExtra:void 0,style:E?void 0:{display:"contents"},children:(0,Xe.jsx)("div",{className:E?Re.sharedExtraInner:void 0,style:E?void 0:{display:"contents"},children:(0,Xe.jsxs)("div",{className:Re.quote,children:["\u201C",o.slice(0,80),o.length>80?"...":"","\u201D"]})})}),r&&(0,Xe.jsx)("img",{className:Re.screenshot,src:r,alt:"Screenshot sent with this note",title:"This picture is sent with the note",draggable:!1}),(0,Xe.jsx)("div",{"data-shared-note":!0,className:E?Re.sharedNote:void 0,style:E?{"--field-border":D?k:void 0}:{display:"contents"},children:(0,Xe.jsx)("div",{className:E?Re.sharedNoteContent:void 0,style:E?void 0:{display:"contents"},children:(0,Xe.jsx)("textarea",{ref:O,className:Re.textarea,readOnly:S,"aria-hidden":S,style:E?void 0:{borderColor:D?k:void 0},placeholder:i,value:S?F.replace(/\s+/g," "):F,onChange:K=>W(K.target.value),onFocus:()=>j(!0),onBlur:()=>j(!1),rows:2,onKeyDown:Ce})})}),(0,Xe.jsx)("div",{"data-editor-actions":!0,className:E?Re.sharedActions:void 0,style:E?void 0:{display:"contents"},children:(0,Xe.jsxs)("div",{className:Re.actions,children:[d&&(0,Xe.jsx)("div",{className:Re.deleteWrapper,children:(0,Xe.jsx)("button",{className:Re.deleteButton,onClick:d,type:"button","aria-label":"Delete annotation",children:"Delete"})}),(0,Xe.jsx)("button",{className:Re.cancel,onClick:c,children:"Cancel"}),(0,Xe.jsx)("button",{className:Re.submit,style:{backgroundColor:k,opacity:F.trim()||f?1:.4},onClick:J,disabled:C||!F.trim()&&!f,children:l})]})})]})});var pm=ce(ze()),is=(0,Qt.forwardRef)(function({element:t,timestamp:n,selectedText:o,screenshot:r,placeholder:i="What should change?",initialValue:s="",submitLabel:l="Add",onSubmit:a,onCancel:c,onDelete:d,onOpenSource:m,allowEmpty:f=!1,style:k,accentColor:w="#3c82f7",isExiting:C=!1,onExitComplete:S,lightMode:g=!1,computedStyles:x},h){let[E,F]=(0,Qt.useState)(!1),[W,D]=(0,Qt.useState)("initial"),j=(0,Qt.useRef)(null),Z=(0,Qt.useRef)(null);(0,Qt.useEffect)(()=>{mm(Z.current?.getRootNode(),"annotation-popup",Wc)},[]);let q=(0,Qt.useRef)(null);(0,Qt.useEffect)(()=>{let Ce=De(()=>{D(K=>K==="initial"?"enter":K)},0);return()=>{clearTimeout(Ce),q.current&&clearTimeout(q.current)}},[]),(0,Qt.useEffect)(()=>{if(C)return;let Ce=De(()=>j.current?.focus(),50);return()=>clearTimeout(Ce)},[C]);let O=(0,Qt.useCallback)(()=>{q.current&&clearTimeout(q.current),F(!0),q.current=De(()=>{F(!1),j.current?.focus()},250)},[]);(0,Qt.useImperativeHandle)(h,()=>({shake:O}),[O]);let Ae=(0,Qt.useCallback)(()=>{if(S){c();return}D("exit")},[c,S]),ue=C?"exit":W;rs(Z,ue==="exit",()=>{C?S?.():c()});let J=[Re.popup,g?Re.light:"",ue==="enter"?Re.enter:"",ue==="entered"?Re.entered:"",ue==="exit"?Re.exit:"",E&&ue!=="exit"?Re.shake:""].filter(Boolean).join(" ");return(0,pm.jsx)("div",{ref:Z,className:J,"data-annotation-popup":!0,style:k,onAnimationEnd:Ce=>{Ce.target===Ce.currentTarget&&Ce.animationName.includes("popupEnter")&&!C&&D("entered")},onKeyDownCapture:Ce=>{Ce.key!=="Escape"||Ce.nativeEvent.isComposing||(Ce.preventDefault(),Ce.stopPropagation(),Ae())},onClick:Ce=>Ce.stopPropagation(),children:(0,pm.jsx)(jc,{ref:j,element:t,timestamp:n,selectedText:o,screenshot:r,placeholder:i,initialValue:s,submitLabel:l,onSubmit:a,onCancel:Ae,onDelete:d,onOpenSource:m,allowEmpty:f,accentColor:w,computedStyles:x,disabled:ue==="exit"})})});var ss=`.icon-transitions-module__iconState___uqK9J {
  transition: opacity 0.2s ease, transform 0.2s ease;
  transform-origin: center;
}

.icon-transitions-module__iconStateFast___HxlMm {
  transition: opacity 0.15s ease, transform 0.15s ease;
  transform-origin: center;
}

.icon-transitions-module__iconFade___nPwXg {
  transition: opacity 0.2s ease;
}

.icon-transitions-module__iconFadeFast___Ofb2t {
  transition: opacity 0.15s ease;
}

.icon-transitions-module__visible___PlHsU {
  opacity: 1 !important;
}

.icon-transitions-module__visibleScaled___8Qog- {
  opacity: 1 !important;
  transform: scale(1);
}

.icon-transitions-module__hidden___ETykt {
  opacity: 0 !important;
}

.icon-transitions-module__hiddenScaled___JXn-m {
  opacity: 0 !important;
  transform: scale(0.8);
}

.icon-transitions-module__sending___uaLN- {
  opacity: 0.5 !important;
  transform: scale(0.8);
}`,pt={iconState:"icon-transitions-module__iconState___uqK9J",iconStateFast:"icon-transitions-module__iconStateFast___HxlMm",iconFade:"icon-transitions-module__iconFade___nPwXg",iconFadeFast:"icon-transitions-module__iconFadeFast___Ofb2t",visible:"icon-transitions-module__visible___PlHsU",visibleScaled:"icon-transitions-module__visibleScaled___8Qog-",hidden:"icon-transitions-module__hidden___ETykt",hiddenScaled:"icon-transitions-module__hiddenScaled___JXn-m",sending:"icon-transitions-module__sending___uaLN-"};var ne=ce(ze());var G0=({size:e=16})=>(0,ne.jsx)("svg",{width:e,height:e,viewBox:"0 0 16 16",fill:"none",children:(0,ne.jsx)("path",{d:"M8 3v10M3 8h10",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"})});var q0=({size:e=20,...t})=>(0,ne.jsxs)("svg",{width:e,height:e,viewBox:"0 0 20 20",fill:"none",xmlns:"http://www.w3.org/2000/svg",...t,children:[(0,ne.jsx)("circle",{cx:"10",cy:"10",r:"5.375",stroke:"currentColor",strokeWidth:"1.25"}),(0,ne.jsx)("path",{d:"M8.5 8.5C8.73 7.85 9.31 7.49 10 7.5C10.86 7.51 11.5 8.13 11.5 9C11.5 10.08 10 10.5 10 10.5V10.75",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("circle",{cx:"10",cy:"12.625",r:"0.625",fill:"currentColor"})]});var K0=({size:e=24,copied:t=!1,tint:n})=>(0,ne.jsxs)("svg",{ref:Uc("icon-transitions",ss),width:e,height:e,viewBox:"0 0 24 24",fill:"none",style:n?{color:n,transition:"color 0.3s ease"}:void 0,children:[(0,ne.jsxs)("g",{className:`${pt.iconState} ${t?pt.hiddenScaled:pt.visibleScaled}`,children:[(0,ne.jsx)("path",{d:"M4.75 11.25C4.75 10.4216 5.42157 9.75 6.25 9.75H12.75C13.5784 9.75 14.25 10.4216 14.25 11.25V17.75C14.25 18.5784 13.5784 19.25 12.75 19.25H6.25C5.42157 19.25 4.75 18.5784 4.75 17.75V11.25Z",stroke:"currentColor",strokeWidth:"1.5"}),(0,ne.jsx)("path",{d:"M17.25 14.25H17.75C18.5784 14.25 19.25 13.5784 19.25 12.75V6.25C19.25 5.42157 18.5784 4.75 17.75 4.75H11.25C10.4216 4.75 9.75 5.42157 9.75 6.25V6.75",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"})]}),(0,ne.jsxs)("g",{className:`${pt.iconState} ${t?pt.visibleScaled:pt.hiddenScaled}`,children:[(0,ne.jsx)("path",{d:"M12 20C7.58172 20 4 16.4182 4 12C4 7.58172 7.58172 4 12 4C16.4182 4 20 7.58172 20 12C20 16.4182 16.4182 20 12 20Z",stroke:"var(--agentation-color-green)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M15 10L11 14.25L9.25 12.25",stroke:"var(--agentation-color-green)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})]})]}),J0=({size:e=24,state:t="idle"})=>{let n=t==="idle",o=t==="sent",r=t==="failed",i=t==="sending";return(0,ne.jsxs)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,ne.jsx)("g",{className:`${pt.iconStateFast} ${n?pt.visibleScaled:i?pt.sending:pt.hiddenScaled}`,children:(0,ne.jsx)("path",{d:"M9.875 14.125L12.3506 19.6951C12.7184 20.5227 13.9091 20.4741 14.2083 19.6193L18.8139 6.46032C19.0907 5.6695 18.3305 4.90933 17.5397 5.18611L4.38072 9.79174C3.52589 10.0909 3.47731 11.2816 4.30494 11.6494L9.875 14.125ZM9.875 14.125L13.375 10.625",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})}),(0,ne.jsxs)("g",{className:`${pt.iconStateFast} ${o?pt.visibleScaled:pt.hiddenScaled}`,children:[(0,ne.jsx)("path",{d:"M12 20C7.58172 20 4 16.4182 4 12C4 7.58172 7.58172 4 12 4C16.4182 4 20 7.58172 20 12C20 16.4182 16.4182 20 12 20Z",stroke:"var(--agentation-color-green)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M15 10L11 14.25L9.25 12.25",stroke:"var(--agentation-color-green)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})]}),(0,ne.jsxs)("g",{className:`${pt.iconStateFast} ${r?pt.visibleScaled:pt.hiddenScaled}`,children:[(0,ne.jsx)("path",{d:"M12 20C7.58172 20 4 16.4182 4 12C4 7.58172 7.58172 4 12 4C16.4182 4 20 7.58172 20 12C20 16.4182 16.4182 20 12 20Z",stroke:"var(--agentation-color-red)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M12 8V12",stroke:"var(--agentation-color-red)",strokeWidth:"1.5",strokeLinecap:"round"}),(0,ne.jsx)("circle",{cx:"12",cy:"15",r:"0.5",fill:"var(--agentation-color-red)",stroke:"var(--agentation-color-red)",strokeWidth:"1"})]})]})};var Z0=({size:e=24,isOpen:t=!0})=>(0,ne.jsxs)("svg",{ref:Uc("icon-transitions",ss),width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,ne.jsxs)("g",{className:`${pt.iconFade} ${t?pt.visible:pt.hidden}`,children:[(0,ne.jsx)("path",{d:"M3.91752 12.7539C3.65127 12.2996 3.65037 11.7515 3.9149 11.2962C4.9042 9.59346 7.72688 5.49994 12 5.49994C16.2731 5.49994 19.0958 9.59346 20.0851 11.2962C20.3496 11.7515 20.3487 12.2996 20.0825 12.7539C19.0908 14.4459 16.2694 18.4999 12 18.4999C7.73064 18.4999 4.90918 14.4459 3.91752 12.7539Z",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M12 14.8261C13.5608 14.8261 14.8261 13.5608 14.8261 12C14.8261 10.4392 13.5608 9.17392 12 9.17392C10.4392 9.17392 9.17391 10.4392 9.17391 12C9.17391 13.5608 10.4392 14.8261 12 14.8261Z",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})]}),(0,ne.jsxs)("g",{className:`${pt.iconFade} ${t?pt.hidden:pt.visible}`,children:[(0,ne.jsx)("path",{d:"M18.6025 9.28503C18.9174 8.9701 19.4364 8.99481 19.7015 9.35271C20.1484 9.95606 20.4943 10.507 20.7342 10.9199C21.134 11.6086 21.1329 12.4454 20.7303 13.1328C20.2144 14.013 19.2151 15.5225 17.7723 16.8193C16.3293 18.1162 14.3852 19.2497 12.0008 19.25C11.4192 19.25 10.8638 19.1823 10.3355 19.0613C9.77966 18.934 9.63498 18.2525 10.0382 17.8493C10.2412 17.6463 10.5374 17.573 10.8188 17.6302C11.1993 17.7076 11.5935 17.75 12.0008 17.75C13.8848 17.7497 15.4867 16.8568 16.7693 15.7041C18.0522 14.5511 18.9606 13.1867 19.4363 12.375C19.5656 12.1543 19.5659 11.8943 19.4373 11.6729C19.2235 11.3049 18.921 10.8242 18.5364 10.3003C18.3085 9.98991 18.3302 9.5573 18.6025 9.28503ZM12.0008 4.75C12.5814 4.75006 13.1358 4.81803 13.6632 4.93953C14.2182 5.06741 14.362 5.74812 13.9593 6.15091C13.7558 6.35435 13.4589 6.42748 13.1771 6.36984C12.7983 6.29239 12.4061 6.25006 12.0008 6.25C10.1167 6.25 8.51415 7.15145 7.23028 8.31543C5.94678 9.47919 5.03918 10.8555 4.56426 11.6729C4.43551 11.8945 4.43582 12.1542 4.56524 12.375C4.77587 12.7343 5.07189 13.2012 5.44718 13.7105C5.67623 14.0213 5.65493 14.4552 5.38193 14.7282C5.0671 15.0431 4.54833 15.0189 4.28292 14.6614C3.84652 14.0736 3.50813 13.5369 3.27129 13.1328C2.86831 12.4451 2.86717 11.6088 3.26739 10.9199C3.78185 10.0345 4.77959 8.51239 6.22247 7.2041C7.66547 5.89584 9.61202 4.75 12.0008 4.75Z",fill:"currentColor"}),(0,ne.jsx)("path",{d:"M5 19L19 5",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"})]})]}),ey=({size:e=24,isPaused:t=!1})=>(0,ne.jsxs)("svg",{ref:Uc("icon-transitions",ss),width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,ne.jsxs)("g",{className:`${pt.iconFadeFast} ${t?pt.hidden:pt.visible}`,children:[(0,ne.jsx)("path",{d:"M8 6L8 18",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"}),(0,ne.jsx)("path",{d:"M16 18L16 6",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"})]}),(0,ne.jsx)("path",{className:`${pt.iconFadeFast} ${t?pt.visible:pt.hidden}`,d:"M17.75 10.701C18.75 11.2783 18.75 12.7217 17.75 13.299L8.75 18.4952C7.75 19.0725 6.5 18.3509 6.5 17.1962L6.5 6.80384C6.5 5.64914 7.75 4.92746 8.75 5.50481L17.75 10.701Z",stroke:"currentColor",strokeWidth:"1.5"})]});var ty=({size:e=16})=>(0,ne.jsxs)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,ne.jsx)("path",{d:"M10.6504 5.81117C10.9939 4.39628 13.0061 4.39628 13.3496 5.81117C13.5715 6.72517 14.6187 7.15891 15.4219 6.66952C16.6652 5.91193 18.0881 7.33479 17.3305 8.57815C16.8411 9.38134 17.2748 10.4285 18.1888 10.6504C19.6037 10.9939 19.6037 13.0061 18.1888 13.3496C17.2748 13.5715 16.8411 14.6187 17.3305 15.4219C18.0881 16.6652 16.6652 18.0881 15.4219 17.3305C14.6187 16.8411 13.5715 17.2748 13.3496 18.1888C13.0061 19.6037 10.9939 19.6037 10.6504 18.1888C10.4285 17.2748 9.38135 16.8411 8.57815 17.3305C7.33479 18.0881 5.91193 16.6652 6.66952 15.4219C7.15891 14.6187 6.72517 13.5715 5.81117 13.3496C4.39628 13.0061 4.39628 10.9939 5.81117 10.6504C6.72517 10.4285 7.15891 9.38134 6.66952 8.57815C5.91193 7.33479 7.33479 5.91192 8.57815 6.66952C9.38135 7.15891 10.4285 6.72517 10.6504 5.81117Z",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("circle",{cx:"12",cy:"12",r:"2.5",stroke:"currentColor",strokeWidth:"1.5"})]});var ny=({size:e=16})=>(0,ne.jsx)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:(0,ne.jsx)("path",{d:"M13.5 4C14.7426 4 15.75 5.00736 15.75 6.25V7H18.5C18.9142 7 19.25 7.33579 19.25 7.75C19.25 8.16421 18.9142 8.5 18.5 8.5H17.9678L17.6328 16.2217C17.61 16.7475 17.5912 17.1861 17.5469 17.543C17.5015 17.9087 17.4225 18.2506 17.2461 18.5723C16.9747 19.0671 16.5579 19.4671 16.0518 19.7168C15.7227 19.8791 15.3772 19.9422 15.0098 19.9717C14.6514 20.0004 14.2126 20 13.6865 20H10.3135C9.78735 20 9.34856 20.0004 8.99023 19.9717C8.62278 19.9422 8.27729 19.8791 7.94824 19.7168C7.44205 19.4671 7.02532 19.0671 6.75391 18.5723C6.57751 18.2506 6.49853 17.9087 6.45312 17.543C6.40883 17.1861 6.39005 16.7475 6.36719 16.2217L6.03223 8.5H5.5C5.08579 8.5 4.75 8.16421 4.75 7.75C4.75 7.33579 5.08579 7 5.5 7H8.25V6.25C8.25 5.00736 9.25736 4 10.5 4H13.5ZM7.86621 16.1562C7.89013 16.7063 7.90624 17.0751 7.94141 17.3584C7.97545 17.6326 8.02151 17.7644 8.06934 17.8516C8.19271 18.0763 8.38239 18.2577 8.6123 18.3711C8.70153 18.4151 8.83504 18.4545 9.11035 18.4766C9.39482 18.4994 9.76335 18.5 10.3135 18.5H13.6865C14.2367 18.5 14.6052 18.4994 14.8896 18.4766C15.165 18.4545 15.2985 18.4151 15.3877 18.3711C15.6176 18.2577 15.8073 18.0763 15.9307 17.8516C15.9785 17.7644 16.0245 17.6326 16.0586 17.3584C16.0938 17.0751 16.1099 16.7063 16.1338 16.1562L16.4668 8.5H7.5332L7.86621 16.1562ZM9.97656 10.75C10.3906 10.7371 10.7371 11.0626 10.75 11.4766L10.875 15.4766C10.8879 15.8906 10.5624 16.2371 10.1484 16.25C9.73443 16.2629 9.38794 15.9374 9.375 15.5234L9.25 11.5234C9.23706 11.1094 9.56255 10.7629 9.97656 10.75ZM14.0244 10.75C14.4384 10.7635 14.7635 11.1105 14.75 11.5244L14.6201 15.5244C14.6066 15.9384 14.2596 16.2634 13.8457 16.25C13.4317 16.2365 13.1067 15.8896 13.1201 15.4756L13.251 11.4756C13.2645 11.0617 13.6105 10.7366 14.0244 10.75ZM10.5 5.5C10.0858 5.5 9.75 5.83579 9.75 6.25V7H14.25V6.25C14.25 5.83579 13.9142 5.5 13.5 5.5H10.5Z",fill:"currentColor"})});var oy=({size:e=16})=>(0,ne.jsxs)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,ne.jsxs)("g",{clipPath:"url(#clip0_2_53)",children:[(0,ne.jsx)("path",{d:"M16.25 16.25L7.75 7.75",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M7.75 16.25L16.25 7.75",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})]}),(0,ne.jsx)("defs",{children:(0,ne.jsx)("clipPath",{id:"clip0_2_53",children:(0,ne.jsx)("rect",{width:"24",height:"24",fill:"white"})})})]});var ry=({size:e=16})=>(0,ne.jsxs)("svg",{width:e,height:e,viewBox:"0 0 20 20",fill:"none",children:[(0,ne.jsx)("path",{d:"M9.99999 12.7082C11.4958 12.7082 12.7083 11.4956 12.7083 9.99984C12.7083 8.50407 11.4958 7.2915 9.99999 7.2915C8.50422 7.2915 7.29166 8.50407 7.29166 9.99984C7.29166 11.4956 8.50422 12.7082 9.99999 12.7082Z",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M10 3.9585V5.05698",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M10 14.9429V16.0414",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M5.7269 5.72656L6.50682 6.50649",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M13.4932 13.4932L14.2731 14.2731",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M3.95834 10H5.05683",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M14.9432 10H16.0417",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M5.7269 14.2731L6.50682 13.4932",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,ne.jsx)("path",{d:"M13.4932 6.50649L14.2731 5.72656",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"})]}),iy=({size:e=16})=>(0,ne.jsx)("svg",{width:e,height:e,viewBox:"0 0 20 20",fill:"none",children:(0,ne.jsx)("path",{d:"M15.5 10.4955C15.4037 11.5379 15.0124 12.5314 14.3721 13.3596C13.7317 14.1878 12.8688 14.8165 11.8841 15.1722C10.8995 15.5278 9.83397 15.5957 8.81217 15.3679C7.79038 15.1401 6.8546 14.6259 6.11434 13.8857C5.37408 13.1454 4.85995 12.2096 4.63211 11.1878C4.40427 10.166 4.47215 9.10048 4.82781 8.11585C5.18346 7.13123 5.81218 6.26825 6.64039 5.62791C7.4686 4.98756 8.46206 4.59634 9.5045 4.5C8.89418 5.32569 8.60049 6.34302 8.67685 7.36695C8.75321 8.39087 9.19454 9.35339 9.92058 10.0794C10.6466 10.8055 11.6091 11.2468 12.6331 11.3231C13.657 11.3995 14.6743 11.1058 15.5 10.4955Z",stroke:"currentColor",strokeWidth:"1.13793",strokeLinecap:"round",strokeLinejoin:"round"})}),sy=({size:e=16})=>(0,ne.jsx)("svg",{width:e,height:e,viewBox:"0 0 16 16",fill:"none",xmlns:"http://www.w3.org/2000/svg",children:(0,ne.jsx)("path",{d:"M11.3799 6.9572L9.05645 4.63375M11.3799 6.9572L6.74949 11.5699C6.61925 11.6996 6.45577 11.791 6.277 11.8339L4.29549 12.3092C3.93194 12.3964 3.60478 12.0683 3.69297 11.705L4.16585 9.75693C4.20893 9.57947 4.29978 9.4172 4.42854 9.28771L9.05645 4.63375M11.3799 6.9572L12.3455 5.98759C12.9839 5.34655 12.9839 4.31002 12.3455 3.66897C11.7033 3.02415 10.6594 3.02415 10.0172 3.66897L9.06126 4.62892L9.05645 4.63375",stroke:"currentColor",strokeWidth:"0.9",strokeLinecap:"round",strokeLinejoin:"round"})});var ly=({size:e=16})=>(0,ne.jsx)("svg",{width:e,height:e,viewBox:"0 0 16 16",fill:"none",xmlns:"http://www.w3.org/2000/svg",children:(0,ne.jsx)("path",{d:"M8.5 3.5L4 8L8.5 12.5",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})});var ay=({size:e=24})=>(0,ne.jsxs)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,ne.jsx)("rect",{x:"3",y:"3",width:"18",height:"18",rx:"2",stroke:"currentColor",strokeWidth:"1.5"}),(0,ne.jsx)("line",{x1:"3",y1:"9",x2:"21",y2:"9",stroke:"currentColor",strokeWidth:"1.5"}),(0,ne.jsx)("line",{x1:"9",y1:"9",x2:"9",y2:"21",stroke:"currentColor",strokeWidth:"1.5"})]});var Lo=ce(vt()),cy=ce(bl());var ii=ce(ze()),uy=({content:e,children:t,...n})=>{let[o,r]=(0,Lo.useState)(!1),[i,s]=(0,Lo.useState)(!1),[l,a]=(0,Lo.useState)({top:0,right:0}),c=(0,Lo.useRef)(null),d=(0,Lo.useRef)(null),m=(0,Lo.useRef)(null),f=()=>{if(c.current){let C=c.current.getBoundingClientRect();a({top:C.top+C.height/2,right:window.innerWidth-C.left+8})}},k=()=>{s(!0),m.current&&(clearTimeout(m.current),m.current=null),f(),d.current=De(()=>{r(!0)},500)},w=()=>{d.current&&(clearTimeout(d.current),d.current=null),r(!1),m.current=De(()=>{s(!1)},150)};return(0,Lo.useEffect)(()=>()=>{d.current&&clearTimeout(d.current),m.current&&clearTimeout(m.current)},[]),(0,ii.jsxs)(ii.Fragment,{children:[(0,ii.jsx)("span",{ref:c,onMouseEnter:k,onMouseLeave:w,...n,children:t}),i&&(0,cy.createPortal)((0,ii.jsx)("div",{"data-feedback-toolbar":!0,style:{position:"fixed",top:l.top,right:l.right,transform:"translateY(-50%)",padding:"6px 10px",background:"#383838",color:"rgba(255, 255, 255, 0.7)",fontSize:"11px",fontWeight:400,lineHeight:"14px",borderRadius:"10px",width:"180px",textAlign:"left",zIndex:100020,pointerEvents:"none",boxShadow:"0px 1px 8px rgba(0, 0, 0, 0.28)",opacity:o?1:0,transition:"opacity 0.15s ease"},children:e}),document.body)]})};var dy=`.styles-module__tooltip___mcXL2 {
  display: flex;
  justify-content: center;
  align-items: center;
  cursor: help;
}

.styles-module__tooltipIcon___Nq2nD {
  transform: translateY(0.5px);
  color: #fff;
  opacity: 0.2;
  transition: opacity 0.15s ease;
  will-change: transform;
}
.styles-module__tooltip___mcXL2:hover .styles-module__tooltipIcon___Nq2nD {
  opacity: 0.5;
}
[data-agentation-theme=light] .styles-module__tooltipIcon___Nq2nD {
  color: #000;
}`,fm={tooltip:"styles-module__tooltip___mcXL2",tooltipIcon:"styles-module__tooltipIcon___Nq2nD"};var gm=ce(ze()),Ro=({content:e})=>(0,gm.jsx)(uy,{className:fm.tooltip,content:e,children:(0,gm.jsx)(q0,{className:fm.tooltipIcon})});var _y=`.styles-module__toolbar___wNsdK svg[fill=none],
.styles-module__markersLayer___-25j1 svg[fill=none],
.styles-module__fixedMarkersLayer___ffyX6 svg[fill=none] {
  fill: none !important;
}
.styles-module__toolbar___wNsdK svg[fill=none] :not([fill]),
.styles-module__markersLayer___-25j1 svg[fill=none] :not([fill]),
.styles-module__fixedMarkersLayer___ffyX6 svg[fill=none] :not([fill]) {
  fill: none !important;
}

.styles-module__controlsContent___9GJWU :where(button, input, select, textarea, label) {
  background: unset;
  border: unset;
  border-radius: unset;
  padding: unset;
  margin: unset;
  color: unset;
  font-family: unset;
  font-weight: unset;
  font-style: unset;
  line-height: unset;
  letter-spacing: unset;
  text-transform: unset;
  text-decoration: unset;
  box-shadow: unset;
  outline: unset;
}

@keyframes styles-module__toolbarEnter___u8RRu {
  from {
    opacity: 0;
    transform: scale(0.5) rotate(90deg);
  }
  to {
    opacity: 1;
    transform: scale(1) rotate(0deg);
  }
}
@keyframes styles-module__toolbarHide___y8kaT {
  from {
    opacity: 1;
    transform: scale(1);
  }
  to {
    opacity: 0;
    transform: scale(0.8);
  }
}
@keyframes styles-module__badgeEnter___mVQLj {
  from {
    opacity: 0;
    transform: scale(0);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
@keyframes styles-module__scaleIn___c-r1K {
  from {
    opacity: 0;
    transform: scale(0.85);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
@keyframes styles-module__scaleOut___Wctwz {
  from {
    opacity: 1;
    transform: scale(1);
  }
  to {
    opacity: 0;
    transform: scale(0.85);
  }
}
@keyframes styles-module__slideUp___kgD36 {
  from {
    opacity: 0;
    transform: scale(0.85) translateY(8px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
@keyframes styles-module__slideDown___zcdje {
  from {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
  to {
    opacity: 0;
    transform: scale(0.85) translateY(8px);
  }
}
@keyframes styles-module__fadeIn___b9qmf {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
@keyframes styles-module__fadeOut___6Ut6- {
  from {
    opacity: 1;
  }
  to {
    opacity: 0;
  }
}
@keyframes styles-module__hoverHighlightIn___6WYHY {
  from {
    opacity: 0;
    transform: scale(0.98);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
@keyframes styles-module__hoverTooltipIn___FYGQx {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(4px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
.styles-module__disableTransitions___EopxO :is(*, *::before, *::after) {
  transition: none !important;
}

:host {
  /* Set here rather than inline so a consumer className rule can still hide the toolbar. */
  display: contents;
  position: fixed;
  top: auto;
  left: auto;
  bottom: 1.25rem;
  right: 1.25rem;
  z-index: 100000;
}

.styles-module__positionContext___AZFHE,
.styles-module__toolbar___wNsdK {
  position: inherit;
  top: inherit;
  left: inherit;
  bottom: inherit;
  right: inherit;
  z-index: inherit;
}

.styles-module__toolbar___wNsdK {
  width: 337px;
  font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  pointer-events: none;
  transition: left 0.36s cubic-bezier(0.19, 1, 0.22, 1), top 0s, right 0s, bottom 0s;
}
.styles-module__toolbar___wNsdK[data-dragging=true] {
  transition: none;
}

.styles-module__toolbarContainer___dIhma {
  position: relative;
  -webkit-user-select: none;
  user-select: none;
  margin-left: auto;
  align-self: flex-end;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #1a1a1a;
  color: #fff;
  border: none;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2), 0 4px 16px rgba(0, 0, 0, 0.1);
  pointer-events: auto;
  transition: width 0.36s cubic-bezier(0.19, 1, 0.22, 1), transform 0.36s cubic-bezier(0.19, 1, 0.22, 1);
}
.styles-module__toolbarContainer___dIhma.styles-module__entrance___sgHd8 {
  animation: styles-module__toolbarEnter___u8RRu 0.5s cubic-bezier(0.34, 1.2, 0.64, 1) forwards;
}
.styles-module__toolbarContainer___dIhma.styles-module__hiding___1td44 {
  animation: styles-module__toolbarHide___y8kaT 0.4s cubic-bezier(0.4, 0, 1, 1) forwards;
  pointer-events: none;
}
.styles-module__toolbarContainer___dIhma.styles-module__collapsed___Rydsn {
  width: 44px;
  height: 44px;
  border-radius: 22px;
  padding: 0;
  cursor: pointer;
}
.styles-module__toolbarContainer___dIhma.styles-module__collapsed___Rydsn:hover {
  background: #2a2a2a;
}
.styles-module__toolbarContainer___dIhma.styles-module__collapsed___Rydsn:active {
  transform: scale(0.95);
}
.styles-module__toolbarContainer___dIhma.styles-module__expanded___ofKPx {
  height: 44px;
  border-radius: 22px;
  padding: 5px;
  width: 297px;
}
.styles-module__toolbarContainer___dIhma.styles-module__expanded___ofKPx.styles-module__serverConnected___Gfbou {
  width: 337px;
}

@media (prefers-reduced-motion: reduce) {
  .styles-module__toolbar___wNsdK,
  .styles-module__toolbarContainer___dIhma {
    transition: none;
  }
}
.styles-module__buttonWrapper___rBcdv.styles-module__toggleWrapper___7N0-q {
  position: absolute;
  top: 0;
  right: 0;
  width: 44px;
  height: 44px;
}

.styles-module__togglePlaceholder___wnqrL {
  width: 34px;
  flex: 0 0 34px;
  height: 34px;
}

.styles-module__toggleContent___0yfyP {
  position: absolute;
  top: 0;
  right: 0;
  width: 44px;
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 0;
  border-radius: 50%;
  padding: 0;
  margin: 0;
  background: transparent;
  color: #fff;
  cursor: pointer;
  transition: color 0.15s ease;
}
.styles-module__toggleContent___0yfyP::before {
  content: "";
  position: absolute;
  inset: 5px;
  border-radius: 50%;
  pointer-events: none;
  background: transparent;
  transition: background-color 0.15s ease, transform 0.1s ease;
}
.styles-module__toggleContent___0yfyP.styles-module__expandedToggle___F7SRN {
  color: rgba(255, 255, 255, 0.85);
}
.styles-module__toggleContent___0yfyP.styles-module__expandedToggle___F7SRN:hover {
  color: #fff;
}
.styles-module__toggleContent___0yfyP.styles-module__expandedToggle___F7SRN:hover::before {
  background: rgba(255, 255, 255, 0.12);
}
.styles-module__toggleContent___0yfyP.styles-module__expandedToggle___F7SRN:active::before, .styles-module__toggleContent___0yfyP.styles-module__expandedToggle___F7SRN:active .styles-module__toggleGlyph___R7Oom {
  transform: scale(0.92);
}

.styles-module__toggleIcon___Jbtus {
  transform: translateY(-0.5px);
  transition: transform 0.3s cubic-bezier(0.22, 1, 0.36, 1);
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}

.styles-module__expandedToggle___F7SRN .styles-module__toggleIcon___Jbtus {
  transform: none;
}

.styles-module__toggleGlyph___R7Oom {
  overflow: visible;
  transition: transform 0.1s ease;
}
.styles-module__toggleGlyph___R7Oom path {
  transform-box: fill-box;
  transform-origin: center;
  transition: transform 0.3s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.16s ease;
}

.styles-module__toggleTopLine___hQaCm,
.styles-module__toggleMiddleLine___sFFVe {
  vector-effect: non-scaling-stroke;
}

.styles-module__toggleBottomLine___V-jX3 {
  transform-origin: left center;
}

.styles-module__toggleGlyph___R7Oom[data-active=true] .styles-module__toggleTopLine___hQaCm {
  transform: translateY(5.25px) rotate(45deg) scaleX(1.1422494);
}
.styles-module__toggleGlyph___R7Oom[data-active=true] .styles-module__toggleMiddleLine___sFFVe {
  transform: translateX(3.5px) rotate(-45deg) scaleX(2.4748737);
}
.styles-module__toggleGlyph___R7Oom[data-active=true] .styles-module__toggleBottomLine___V-jX3 {
  transform: scaleX(0);
  opacity: 0;
}
.styles-module__toggleGlyph___R7Oom[data-active=true] .styles-module__toggleSparkle___eeF99 {
  transform: scale(0);
  opacity: 0;
}

.styles-module__toggleContent___0yfyP:focus-visible,
.styles-module__controlButton___8Q0jc:focus-visible {
  outline: 2px solid var(--agentation-color-accent);
  outline-offset: 3px;
}

.styles-module__controlsContent___9GJWU {
  display: flex;
  align-items: center;
  gap: 6px;
  transition: filter 0.14s ease-out, opacity 0.14s ease-out, transform 0.36s cubic-bezier(0.19, 1, 0.22, 1);
}
.styles-module__controlsContent___9GJWU.styles-module__visible___KHwEW {
  transition: filter 0.3s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.24s ease-out, transform 0.42s cubic-bezier(0.19, 1, 0.22, 1);
  opacity: 1;
  filter: blur(0px);
  transform: scale(1);
  visibility: visible;
  pointer-events: auto;
}
.styles-module__controlsContent___9GJWU.styles-module__hidden___Ae8H4 {
  pointer-events: none;
  opacity: 0;
  filter: blur(6px);
  transform: scale(0.4);
}

@media (prefers-reduced-motion: reduce) {
  .styles-module__controlsContent___9GJWU,
  .styles-module__controlsContent___9GJWU.styles-module__visible___KHwEW,
  .styles-module__toggleContent___0yfyP,
  .styles-module__toggleContent___0yfyP::before,
  .styles-module__toggleGlyph___R7Oom,
  .styles-module__toggleIcon___Jbtus,
  .styles-module__toggleGlyph___R7Oom path {
    transition: none;
  }
  .styles-module__controlsContent___9GJWU.styles-module__hidden___Ae8H4 {
    filter: none;
    transform: none;
  }
}
.styles-module__badge___2XsgF {
  position: absolute;
  top: -13px;
  right: -13px;
  -webkit-user-select: none;
  user-select: none;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 9px;
  background-color: var(--agentation-color-accent);
  color: white;
  font-size: 0.625rem;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15), inset 0 0 0 1px rgba(255, 255, 255, 0.04);
  opacity: 1;
  transition: transform 0.3s ease, opacity 0.2s ease;
  transform: scale(1);
}
.styles-module__badge___2XsgF.styles-module__fadeOut___6Ut6- {
  opacity: 0;
  transform: scale(0);
  pointer-events: none;
}
.styles-module__badge___2XsgF.styles-module__entrance___sgHd8 {
  animation: styles-module__badgeEnter___mVQLj 0.3s cubic-bezier(0.34, 1.2, 0.64, 1) 0.4s both;
}

.styles-module__controlButton___8Q0jc {
  position: relative;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: 50%;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.85);
  transition: background-color 0.15s ease, color 0.15s ease, transform 0.1s ease, opacity 0.2s ease;
}
.styles-module__controlButton___8Q0jc:hover:not(:disabled):not([data-active=true]):not([data-failed=true]):not([data-auto-sync=true]):not([data-error=true]):not([data-no-hover=true]) {
  background: rgba(255, 255, 255, 0.12);
  color: #fff;
}
.styles-module__controlButton___8Q0jc:active:not(:disabled) {
  transform: scale(0.92);
}
.styles-module__controlButton___8Q0jc:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
.styles-module__controlButton___8Q0jc[data-active=true] {
  color: var(--agentation-color-blue);
  background-color: color-mix(in srgb, var(--agentation-color-blue) 25%, transparent);
}
.styles-module__controlButton___8Q0jc[data-error=true] {
  color: var(--agentation-color-red);
  background-color: color-mix(in srgb, var(--agentation-color-red) 25%, transparent);
}
.styles-module__controlButton___8Q0jc[data-danger]:hover:not(:disabled):not([data-active=true]):not([data-failed=true]) {
  background-color: color-mix(in srgb, var(--agentation-color-red) 25%, transparent);
  color: var(--agentation-color-red);
}
.styles-module__controlButton___8Q0jc[data-no-hover=true], .styles-module__controlButton___8Q0jc.styles-module__statusShowing___te6iu {
  cursor: default;
  pointer-events: none;
  background: transparent !important;
}
.styles-module__controlButton___8Q0jc[data-auto-sync=true] {
  color: var(--agentation-color-green);
  background: transparent;
  cursor: default;
}
.styles-module__controlButton___8Q0jc[data-failed=true] {
  color: var(--agentation-color-red);
  background-color: color-mix(in srgb, var(--agentation-color-red) 25%, transparent);
}

.styles-module__buttonBadge___NeFWb {
  position: absolute;
  top: 0px;
  right: 0px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: 8px;
  background-color: var(--agentation-color-accent);
  color: white;
  font-size: 0.625rem;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 0 0 2px #1a1a1a, 0 1px 3px rgba(0, 0, 0, 0.2);
  pointer-events: none;
}
[data-agentation-theme=light] .styles-module__buttonBadge___NeFWb {
  box-shadow: 0 0 0 2px #fff, 0 1px 3px rgba(0, 0, 0, 0.2);
}

@keyframes styles-module__mcpIndicatorPulseConnected___EDodZ {
  0%, 100% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-green) 50%, transparent);
  }
  50% {
    box-shadow: 0 0 0 5px color-mix(in srgb, var(--agentation-color-green) 0%, transparent);
  }
}
@keyframes styles-module__mcpIndicatorPulseConnecting___cCYte {
  0%, 100% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-yellow) 50%, transparent);
  }
  50% {
    box-shadow: 0 0 0 5px color-mix(in srgb, var(--agentation-color-yellow) 0%, transparent);
  }
}
.styles-module__mcpIndicator___zGJeL {
  position: absolute;
  top: 3px;
  right: 3px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  pointer-events: none;
  transition: background-color 0.3s ease, opacity 0.15s ease, transform 0.15s ease;
  opacity: 1;
  transform: scale(1);
}
.styles-module__mcpIndicator___zGJeL.styles-module__connected___7c28g {
  background-color: var(--agentation-color-green);
  animation: styles-module__mcpIndicatorPulseConnected___EDodZ 2.5s ease-in-out infinite;
}
.styles-module__mcpIndicator___zGJeL.styles-module__connecting___uo-CW {
  background-color: var(--agentation-color-yellow);
  animation: styles-module__mcpIndicatorPulseConnecting___cCYte 1.5s ease-in-out infinite;
}
.styles-module__mcpIndicator___zGJeL.styles-module__hidden___Ae8H4 {
  opacity: 0;
  transform: scale(0);
  animation: none;
}

@keyframes styles-module__connectionPulse___-Zycw {
  0%, 100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.6;
    transform: scale(0.9);
  }
}
.styles-module__connectionIndicatorWrapper___L-e-3 {
  width: 8px;
  height: 34px;
  margin-left: 6px;
  margin-right: 6px;
}

.styles-module__connectionIndicator___afk9p {
  position: relative;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  opacity: 0;
  transition: opacity 0.3s ease, background-color 0.3s ease;
  cursor: default;
}

.styles-module__connectionIndicatorVisible___C-i5B {
  opacity: 1;
}

.styles-module__connectionIndicatorConnected___IY8pR {
  background-color: var(--agentation-color-green);
  animation: styles-module__connectionPulse___-Zycw 2.5s ease-in-out infinite;
}

.styles-module__connectionIndicatorDisconnected___kmpaZ {
  background-color: var(--agentation-color-red);
  animation: none;
}

.styles-module__connectionIndicatorConnecting___QmSLH {
  background-color: var(--agentation-color-yellow);
  animation: styles-module__connectionPulse___-Zycw 1s ease-in-out infinite;
}

.styles-module__buttonWrapper___rBcdv {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}
.styles-module__buttonWrapper___rBcdv:hover .styles-module__buttonTooltip___Burd9 {
  opacity: 1;
  visibility: visible;
  transform: translateX(-50%) scale(1);
  transition-delay: 0.85s;
}
.styles-module__buttonWrapper___rBcdv:has(.styles-module__controlButton___8Q0jc:disabled):hover .styles-module__buttonTooltip___Burd9 {
  opacity: 0;
  visibility: hidden;
}

.styles-module__tooltipsInSession___-0lHH .styles-module__buttonWrapper___rBcdv:hover .styles-module__buttonTooltip___Burd9 {
  transition-delay: 0s;
}

.styles-module__sendButtonWrapper___UUxG6 {
  width: 0;
  opacity: 0;
  overflow: hidden;
  pointer-events: none;
  margin-left: -6px;
  transition: width 0.36s cubic-bezier(0.19, 1, 0.22, 1), opacity 0.2s cubic-bezier(0.19, 1, 0.22, 1), margin 0.36s cubic-bezier(0.19, 1, 0.22, 1);
}
.styles-module__sendButtonWrapper___UUxG6 .styles-module__controlButton___8Q0jc {
  transform: scale(0.8);
  transition: transform 0.36s cubic-bezier(0.19, 1, 0.22, 1);
}
.styles-module__sendButtonWrapper___UUxG6.styles-module__sendButtonVisible___WPSQU {
  width: 34px;
  opacity: 1;
  overflow: visible;
  pointer-events: auto;
  margin-left: 0;
}
.styles-module__sendButtonWrapper___UUxG6.styles-module__sendButtonVisible___WPSQU .styles-module__controlButton___8Q0jc {
  transform: scale(1);
}

.styles-module__buttonTooltip___Burd9 {
  position: absolute;
  bottom: calc(100% + 14px);
  left: 50%;
  transform: translateX(-50%) scale(0.95);
  padding: 6px 10px;
  background: #1a1a1a;
  color: rgba(255, 255, 255, 0.9);
  font-size: 12px;
  font-weight: 500;
  border-radius: 8px;
  white-space: nowrap;
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  z-index: 100001;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  transition: opacity 0.135s ease, transform 0.135s ease, visibility 0.135s ease;
}
.styles-module__buttonTooltip___Burd9::after {
  content: "";
  position: absolute;
  top: calc(100% - 4px);
  left: 50%;
  transform: translateX(-50%) rotate(45deg);
  width: 8px;
  height: 8px;
  background: #1a1a1a;
  border-radius: 0 0 2px 0;
}

.styles-module__shortcut___lEAQk {
  margin-left: 4px;
  opacity: 0.5;
}

.styles-module__tooltipBelow___m6ats .styles-module__buttonTooltip___Burd9 {
  bottom: auto;
  top: calc(100% + 14px);
  transform: translateX(-50%) scale(0.95);
}
.styles-module__tooltipBelow___m6ats .styles-module__buttonTooltip___Burd9::after {
  top: -4px;
  bottom: auto;
  border-radius: 2px 0 0 0;
}

.styles-module__tooltipBelow___m6ats .styles-module__buttonWrapper___rBcdv:hover .styles-module__buttonTooltip___Burd9 {
  transform: translateX(-50%) scale(1);
}

.styles-module__tooltipsHidden___VtLJG .styles-module__buttonTooltip___Burd9 {
  opacity: 0 !important;
  visibility: hidden !important;
  transition: none !important;
}

.styles-module__tooltipVisible___0jcCv,
.styles-module__tooltipsHidden___VtLJG .styles-module__tooltipVisible___0jcCv {
  opacity: 1 !important;
  visibility: visible !important;
  transform: translateX(-50%) scale(1) !important;
  transition-delay: 0s !important;
}

.styles-module__buttonWrapperAlignLeft___myzIp .styles-module__buttonTooltip___Burd9 {
  left: 50%;
  transform: translateX(-12px) scale(0.95);
}
.styles-module__buttonWrapperAlignLeft___myzIp .styles-module__buttonTooltip___Burd9::after {
  left: 16px;
}
.styles-module__buttonWrapperAlignLeft___myzIp:hover .styles-module__buttonTooltip___Burd9 {
  transform: translateX(-12px) scale(1);
}

.styles-module__tooltipBelow___m6ats .styles-module__buttonWrapperAlignLeft___myzIp .styles-module__buttonTooltip___Burd9 {
  transform: translateX(-12px) scale(0.95);
}
.styles-module__tooltipBelow___m6ats .styles-module__buttonWrapperAlignLeft___myzIp:hover .styles-module__buttonTooltip___Burd9 {
  transform: translateX(-12px) scale(1);
}

.styles-module__buttonWrapperAlignRight___HCQFR .styles-module__buttonTooltip___Burd9 {
  left: 50%;
  transform: translateX(calc(-100% + 12px)) scale(0.95);
}
.styles-module__buttonWrapperAlignRight___HCQFR .styles-module__buttonTooltip___Burd9::after {
  left: auto;
  right: 8px;
}
.styles-module__buttonWrapperAlignRight___HCQFR:hover .styles-module__buttonTooltip___Burd9 {
  transform: translateX(calc(-100% + 12px)) scale(1);
}

.styles-module__tooltipBelow___m6ats .styles-module__buttonWrapperAlignRight___HCQFR .styles-module__buttonTooltip___Burd9 {
  transform: translateX(calc(-100% + 12px)) scale(0.95);
}
.styles-module__tooltipBelow___m6ats .styles-module__buttonWrapperAlignRight___HCQFR:hover .styles-module__buttonTooltip___Burd9 {
  transform: translateX(calc(-100% + 12px)) scale(1);
}

.styles-module__divider___c--s1 {
  width: 1px;
  height: 12px;
  background: rgba(255, 255, 255, 0.15);
  margin: 0 3px;
}

.styles-module__overlay___Q1O9y {
  position: fixed;
  inset: 0;
  z-index: 99997;
  pointer-events: none;
}
.styles-module__overlay___Q1O9y > * {
  pointer-events: auto;
}

.styles-module__hoverHighlight___ogakW {
  position: fixed;
  border: 2px solid color-mix(in srgb, var(--agentation-color-accent) 50%, transparent);
  border-radius: 4px;
  background-color: color-mix(in srgb, var(--agentation-color-accent) 4%, transparent);
  pointer-events: none !important;
  box-sizing: border-box;
  will-change: opacity;
  contain: layout style;
}
.styles-module__hoverHighlight___ogakW.styles-module__enter___WFIki {
  animation: styles-module__hoverHighlightIn___6WYHY 0.12s ease-out forwards;
}

.styles-module__multiSelectOutline___cSJ-m {
  position: fixed;
  border: 2px dashed color-mix(in srgb, var(--agentation-color-green) 60%, transparent);
  border-radius: 4px;
  pointer-events: none !important;
  background-color: color-mix(in srgb, var(--agentation-color-green) 5%, transparent);
  box-sizing: border-box;
  will-change: opacity;
}
.styles-module__multiSelectOutline___cSJ-m.styles-module__enter___WFIki {
  animation: styles-module__fadeIn___b9qmf 0.15s ease-out forwards;
}
.styles-module__multiSelectOutline___cSJ-m.styles-module__exit___fyOJ0 {
  animation: styles-module__fadeOut___6Ut6- 0.15s ease-out forwards;
}

.styles-module__singleSelectOutline___QhX-O {
  position: fixed;
  border: 2px solid color-mix(in srgb, var(--agentation-color-blue) 60%, transparent);
  border-radius: 4px;
  pointer-events: none !important;
  background-color: color-mix(in srgb, var(--agentation-color-blue) 5%, transparent);
  box-sizing: border-box;
  will-change: opacity;
}
.styles-module__singleSelectOutline___QhX-O.styles-module__enter___WFIki {
  animation: styles-module__fadeIn___b9qmf 0.15s ease-out forwards;
}
.styles-module__singleSelectOutline___QhX-O.styles-module__exit___fyOJ0 {
  animation: styles-module__fadeOut___6Ut6- 0.15s ease-out forwards;
}

.styles-module__hoverTooltip___bvLk7 {
  position: fixed;
  z-index: 99999;
  font-size: 0.6875rem;
  font-weight: 500;
  color: #fff;
  background: rgba(0, 0, 0, 0.85);
  padding: 0.35rem 0.6rem;
  border-radius: 0.375rem;
  pointer-events: none !important;
  white-space: nowrap;
  max-width: min(280px, 100vw - 16px - 1.2rem);
  overflow: hidden;
  text-overflow: ellipsis;
}
.styles-module__hoverTooltip___bvLk7.styles-module__enter___WFIki {
  animation: styles-module__hoverTooltipIn___FYGQx 0.1s ease-out forwards;
}

.styles-module__hoverReactPath___gx1IJ {
  font-size: 0.625rem;
  color: rgba(255, 255, 255, 0.6);
  margin-bottom: 0.15rem;
  overflow: hidden;
  text-overflow: ellipsis;
}

.styles-module__hoverElementName___QMLMl {
  overflow: hidden;
  text-overflow: ellipsis;
}

.styles-module__markersLayer___-25j1 {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 0;
  z-index: 99998;
  pointer-events: none;
}
.styles-module__markersLayer___-25j1 > * {
  pointer-events: auto;
}

.styles-module__fixedMarkersLayer___ffyX6 {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 99998;
  pointer-events: none;
}
.styles-module__fixedMarkersLayer___ffyX6 > * {
  pointer-events: auto;
}

.styles-module__marker___6sQrs {
  position: absolute;
  width: 22px;
  height: 22px;
  background: var(--agentation-color-blue);
  color: white;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.6875rem;
  font-weight: 600;
  transform: translate(-50%, -50%) scale(1);
  opacity: 1;
  cursor: pointer;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2), inset 0 0 0 1px rgba(0, 0, 0, 0.04);
  -webkit-user-select: none;
  user-select: none;
  will-change: transform, opacity;
  contain: layout style;
  z-index: 1;
}
.styles-module__marker___6sQrs:hover {
  z-index: 2;
}
.styles-module__marker___6sQrs:not(.styles-module__enter___WFIki):not(.styles-module__exit___fyOJ0):not(.styles-module__clearing___FQ--7) {
  transition: background-color 0.15s ease, transform 0.1s ease;
}
.styles-module__marker___6sQrs.styles-module__enter___WFIki {
  animation: styles-module__markerIn___5FaAP 0.25s cubic-bezier(0.22, 1, 0.36, 1) both;
}
.styles-module__marker___6sQrs.styles-module__exit___fyOJ0 {
  animation: styles-module__markerOut___GU5jX 0.2s ease-out both;
  pointer-events: none;
}
.styles-module__marker___6sQrs.styles-module__clearing___FQ--7 {
  animation: styles-module__markerOut___GU5jX 0.15s ease-out both;
  pointer-events: none;
}
.styles-module__marker___6sQrs:not(.styles-module__enter___WFIki):not(.styles-module__exit___fyOJ0):not(.styles-module__clearing___FQ--7):hover {
  transform: translate(-50%, -50%) scale(1.1);
}
.styles-module__marker___6sQrs.styles-module__pending___2IHLC {
  position: fixed;
  background-color: var(--agentation-color-blue);
  cursor: default;
}
.styles-module__marker___6sQrs.styles-module__fixed___dBMHC {
  position: fixed;
}
.styles-module__marker___6sQrs.styles-module__multiSelect___YWiuz {
  background-color: var(--agentation-color-green);
  width: 26px;
  height: 26px;
  border-radius: 6px;
  font-size: 0.75rem;
}
.styles-module__marker___6sQrs.styles-module__multiSelect___YWiuz.styles-module__pending___2IHLC {
  background-color: var(--agentation-color-green);
}
.styles-module__marker___6sQrs.styles-module__hovered___ZgXIy {
  background-color: var(--agentation-color-red);
}

.styles-module__renumber___nCTxD {
  display: block;
  animation: styles-module__renumberRoll___Wgbq3 0.2s ease-out;
}

@keyframes styles-module__renumberRoll___Wgbq3 {
  0% {
    transform: translateX(-40%);
    opacity: 0;
  }
  100% {
    transform: translateX(0);
    opacity: 1;
  }
}
.styles-module__markerTooltip___aLJID {
  position: absolute;
  top: calc(100% + 10px);
  left: 50%;
  transform: translateX(-50%) scale(0.909);
  z-index: 100002;
  background: #1a1a1a;
  padding: 8px 0.75rem;
  border-radius: 0.75rem;
  font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  font-weight: 400;
  color: #fff;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3), 0 0 0 1px rgba(255, 255, 255, 0.08);
  min-width: 120px;
  max-width: 200px;
  pointer-events: none;
  cursor: default;
}
.styles-module__markerTooltip___aLJID.styles-module__enter___WFIki {
  animation: styles-module__tooltipIn___0N31w 0.1s ease-out forwards;
}

.styles-module__markerQuote___FHmrz {
  display: block;
  font-size: 12px;
  font-style: italic;
  color: rgba(255, 255, 255, 0.6);
  margin-bottom: 0.3125rem;
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.styles-module__markerNote___QkrrS {
  display: block;
  font-size: 13px;
  font-weight: 400;
  line-height: 1.4;
  color: #fff;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  padding-bottom: 2px;
}

.styles-module__markerHint___2iF-6 {
  display: block;
  font-size: 0.625rem;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.6);
  margin-top: 0.375rem;
  white-space: nowrap;
}

.styles-module__settingsPanel___OxX3Y {
  position: absolute;
  right: 5px;
  bottom: calc(100% + 0.5rem);
  z-index: 1;
  overflow: hidden;
  background: #1c1c1c;
  border-radius: 1rem;
  padding: 13px 0 16px;
  min-width: 205px;
  cursor: default;
  opacity: 1;
  box-shadow: 0 1px 8px rgba(0, 0, 0, 0.25), 0 0 0 1px rgba(0, 0, 0, 0.04);
  transition: background-color 0.25s ease, box-shadow 0.25s ease;
}
.styles-module__settingsPanel___OxX3Y::before, .styles-module__settingsPanel___OxX3Y::after {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  width: 16px;
  z-index: 2;
  pointer-events: none;
}
.styles-module__settingsPanel___OxX3Y::before {
  left: 0;
  background: linear-gradient(to right, #1c1c1c 0%, transparent 100%);
}
.styles-module__settingsPanel___OxX3Y::after {
  right: 0;
  background: linear-gradient(to left, #1c1c1c 0%, transparent 100%);
}
.styles-module__settingsPanel___OxX3Y .styles-module__settingsHeader___pwDY9,
.styles-module__settingsPanel___OxX3Y .styles-module__settingsBrand___0gJeM,
.styles-module__settingsPanel___OxX3Y .styles-module__settingsBrandSlash___uTG18,
.styles-module__settingsPanel___OxX3Y .styles-module__settingsVersion___TUcFq,
.styles-module__settingsPanel___OxX3Y .styles-module__settingsSection___m-YM2,
.styles-module__settingsPanel___OxX3Y .styles-module__settingsLabel___8UjfX,
.styles-module__settingsPanel___OxX3Y .styles-module__cycleButton___FMKfw,
.styles-module__settingsPanel___OxX3Y .styles-module__cycleDot___nPgLY,
.styles-module__settingsPanel___OxX3Y .styles-module__dropdownButton___16NPz,
.styles-module__settingsPanel___OxX3Y .styles-module__toggleLabel___Xm8Aa,
.styles-module__settingsPanel___OxX3Y .styles-module__customCheckbox___U39ax,
.styles-module__settingsPanel___OxX3Y .styles-module__sliderLabel___U8sPr,
.styles-module__settingsPanel___OxX3Y .styles-module__slider___GLdxp,
.styles-module__settingsPanel___OxX3Y .styles-module__themeToggle___2rUjA {
  transition: background-color 0.25s ease, color 0.25s ease, border-color 0.25s ease;
}
.styles-module__settingsPanel___OxX3Y.styles-module__enter___WFIki {
  opacity: 1;
  transform: translateY(0) scale(1);
  filter: blur(0px);
  transition: opacity 0.2s ease, transform 0.2s ease, filter 0.2s ease;
}
.styles-module__settingsPanel___OxX3Y.styles-module__exit___fyOJ0 {
  opacity: 0;
  transform: translateY(8px) scale(0.95);
  filter: blur(5px);
  pointer-events: none;
  transition: opacity 0.1s ease, transform 0.1s ease, filter 0.1s ease;
}
[data-agentation-theme=dark] .styles-module__settingsPanel___OxX3Y {
  background: #1a1a1a;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3), 0 0 0 1px rgba(255, 255, 255, 0.08);
}
[data-agentation-theme=dark] .styles-module__settingsPanel___OxX3Y .styles-module__settingsLabel___8UjfX {
  color: rgba(255, 255, 255, 0.6);
}
[data-agentation-theme=dark] .styles-module__settingsPanel___OxX3Y .styles-module__settingsOption___UNa12 {
  color: rgba(255, 255, 255, 0.85);
}
[data-agentation-theme=dark] .styles-module__settingsPanel___OxX3Y .styles-module__settingsOption___UNa12:hover {
  background: rgba(255, 255, 255, 0.1);
}
[data-agentation-theme=dark] .styles-module__settingsPanel___OxX3Y .styles-module__settingsOption___UNa12.styles-module__selected___OwRqP {
  background: rgba(255, 255, 255, 0.15);
  color: #fff;
}
[data-agentation-theme=dark] .styles-module__settingsPanel___OxX3Y .styles-module__toggleLabel___Xm8Aa {
  color: rgba(255, 255, 255, 0.85);
}

.styles-module__settingsPanelContainer___Xksv8 {
  overflow: visible;
  position: relative;
  display: flex;
  padding: 0 1rem;
}

.styles-module__settingsPage___6YfHH {
  min-width: 100%;
  flex-shrink: 0;
  transition: transform 0.2s ease, opacity 0.2s ease;
  transition-delay: 0s;
  opacity: 1;
}

.styles-module__settingsPage___6YfHH.styles-module__slideLeft___Ps01J {
  transform: translateX(-24px);
  opacity: 0;
  pointer-events: none;
}

.styles-module__automationsPage___uvCq6 {
  position: absolute;
  top: 0;
  left: 24px;
  width: 100%;
  height: 100%;
  padding: 3px 1rem 0;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  transition: transform 0.2s ease, opacity 0.2s ease;
  opacity: 0;
  pointer-events: none;
}

.styles-module__automationsPage___uvCq6.styles-module__slideIn___4-qXe {
  transform: translateX(-24px);
  opacity: 1;
  pointer-events: auto;
}

.styles-module__settingsNavLink___wCzJt {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 0;
  border: none;
  background: transparent;
  font-family: inherit;
  font-size: 0.8125rem;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.5);
  cursor: pointer;
  transition: color 0.15s ease;
}
.styles-module__settingsNavLink___wCzJt:hover {
  color: rgba(255, 255, 255, 0.9);
}
[data-agentation-theme=light] .styles-module__settingsNavLink___wCzJt {
  color: rgba(0, 0, 0, 0.5);
}
[data-agentation-theme=light] .styles-module__settingsNavLink___wCzJt:hover {
  color: rgba(0, 0, 0, 0.8);
}
.styles-module__settingsNavLink___wCzJt svg {
  color: rgba(255, 255, 255, 0.4);
  transition: color 0.15s ease;
}
.styles-module__settingsNavLink___wCzJt:hover svg {
  color: #fff;
}
[data-agentation-theme=light] .styles-module__settingsNavLink___wCzJt svg {
  color: rgba(0, 0, 0, 0.25);
}
[data-agentation-theme=light] .styles-module__settingsNavLink___wCzJt:hover svg {
  color: rgba(0, 0, 0, 0.8);
}

.styles-module__settingsNavLinkRight___ZWwhj {
  display: flex;
  align-items: center;
  gap: 6px;
}

.styles-module__mcpNavIndicator___cl9pO {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.styles-module__mcpNavIndicator___cl9pO.styles-module__connected___7c28g {
  background-color: var(--agentation-color-green);
  animation: styles-module__mcpPulse___uNggr 2.5s ease-in-out infinite;
}
.styles-module__mcpNavIndicator___cl9pO.styles-module__connecting___uo-CW {
  background-color: var(--agentation-color-yellow);
  animation: styles-module__mcpPulse___uNggr 1.5s ease-in-out infinite;
}

.styles-module__settingsBackButton___bIe2j {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 0 12px 0;
  margin: -6px 0 0.5rem 0;
  border: none;
  border-bottom: 1px solid rgba(255, 255, 255, 0.07);
  border-radius: 0;
  background: transparent;
  font-family: inherit;
  font-size: 0.8125rem;
  font-weight: 500;
  letter-spacing: -0.15px;
  color: #fff;
  cursor: pointer;
  transition: transform 0.12s cubic-bezier(0.32, 0.72, 0, 1);
}
.styles-module__settingsBackButton___bIe2j svg {
  opacity: 0.4;
  flex-shrink: 0;
  transition: opacity 0.15s ease, transform 0.18s cubic-bezier(0.32, 0.72, 0, 1);
}
.styles-module__settingsBackButton___bIe2j:hover {
  border-bottom-color: rgba(255, 255, 255, 0.07);
}
.styles-module__settingsBackButton___bIe2j:hover svg {
  opacity: 1;
}
[data-agentation-theme=light] .styles-module__settingsBackButton___bIe2j {
  color: rgba(0, 0, 0, 0.85);
  border-bottom-color: rgba(0, 0, 0, 0.08);
}
[data-agentation-theme=light] .styles-module__settingsBackButton___bIe2j:hover {
  border-bottom-color: rgba(0, 0, 0, 0.08);
}

.styles-module__automationHeader___InP0r {
  display: flex;
  align-items: center;
  gap: 0.125rem;
  font-size: 0.8125rem;
  font-weight: 400;
  color: #fff;
}
[data-agentation-theme=light] .styles-module__automationHeader___InP0r {
  color: rgba(0, 0, 0, 0.85);
}

.styles-module__automationDescription___NKlmo {
  font-size: 0.6875rem;
  font-weight: 300;
  color: rgba(255, 255, 255, 0.5);
  margin-top: 2px;
  line-height: 14px;
}
[data-agentation-theme=light] .styles-module__automationDescription___NKlmo {
  color: rgba(0, 0, 0, 0.5);
}

.styles-module__learnMoreLink___8xv-x {
  color: rgba(255, 255, 255, 0.8);
  text-decoration: underline dotted;
  text-decoration-color: rgba(255, 255, 255, 0.2);
  text-underline-offset: 2px;
  transition: color 0.15s ease;
}
.styles-module__learnMoreLink___8xv-x:hover {
  color: #fff;
}
[data-agentation-theme=light] .styles-module__learnMoreLink___8xv-x {
  color: rgba(0, 0, 0, 0.6);
  text-decoration-color: rgba(0, 0, 0, 0.2);
}
[data-agentation-theme=light] .styles-module__learnMoreLink___8xv-x:hover {
  color: rgba(0, 0, 0, 0.85);
}

.styles-module__autoSendRow___UblX5 {
  display: flex;
  align-items: center;
  gap: 8px;
}

.styles-module__autoSendLabel___icDc2 {
  font-size: 0.6875rem;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.4);
  transition: color 0.15s ease;
}
.styles-module__autoSendLabel___icDc2.styles-module__active___-zoN6 {
  color: #66b8ff;
  color: color(display-p3 0.4 0.72 1);
}
[data-agentation-theme=light] .styles-module__autoSendLabel___icDc2 {
  color: rgba(0, 0, 0, 0.4);
}
[data-agentation-theme=light] .styles-module__autoSendLabel___icDc2.styles-module__active___-zoN6 {
  color: var(--agentation-color-blue);
}

.styles-module__webhookUrlInput___2375C {
  display: block;
  width: 100%;
  flex: 1;
  min-height: 60px;
  box-sizing: border-box;
  margin-top: 11px;
  padding: 8px 10px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.03);
  font-family: inherit;
  font-size: 0.75rem;
  font-weight: 400;
  color: #fff;
  outline: none;
  resize: none;
  user-select: text;
  transition: border-color 0.15s ease, background-color 0.15s ease, box-shadow 0.15s ease;
}
.styles-module__webhookUrlInput___2375C::placeholder {
  color: rgba(255, 255, 255, 0.3);
}
.styles-module__webhookUrlInput___2375C:focus {
  border-color: rgba(255, 255, 255, 0.3);
  background: rgba(255, 255, 255, 0.08);
}
[data-agentation-theme=light] .styles-module__webhookUrlInput___2375C {
  border-color: rgba(0, 0, 0, 0.1);
  background: rgba(0, 0, 0, 0.03);
  color: rgba(0, 0, 0, 0.85);
}
[data-agentation-theme=light] .styles-module__webhookUrlInput___2375C::placeholder {
  color: rgba(0, 0, 0, 0.3);
}
[data-agentation-theme=light] .styles-module__webhookUrlInput___2375C:focus {
  border-color: rgba(0, 0, 0, 0.25);
  background: rgba(0, 0, 0, 0.05);
}

.styles-module__settingsHeader___pwDY9 {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 24px;
  margin-bottom: 0.5rem;
  padding-bottom: 9px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.07);
}

.styles-module__settingsBrand___0gJeM {
  font-size: 0.8125rem;
  font-weight: 600;
  letter-spacing: -0.0094em;
  color: #fff;
  text-decoration: none;
}

.styles-module__settingsBrandSlash___uTG18 {
  color: var(--agentation-color-accent);
  transition: color 0.2s ease;
}

.styles-module__settingsVersion___TUcFq {
  font-size: 11px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.4);
  margin-left: auto;
  letter-spacing: -0.0094em;
}

.styles-module__settingsSection___m-YM2 + .styles-module__settingsSection___m-YM2 {
  margin-top: 0.5rem;
  padding-top: 0.5rem;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
}
.styles-module__settingsSection___m-YM2.styles-module__settingsSectionExtraPadding___jdhFV {
  padding-top: calc(0.5rem + 4px);
}

.styles-module__settingsSectionGrow___h-5HZ {
  flex: 1;
  display: flex;
  flex-direction: column;
}

.styles-module__settingsRow___3sdhc {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 24px;
}
.styles-module__settingsRow___3sdhc.styles-module__settingsRowMarginTop___zA0Sp {
  margin-top: 8px;
}

.styles-module__dropdownContainer___BVnxe {
  position: relative;
}

.styles-module__dropdownButton___16NPz {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.25rem 0.5rem;
  border: none;
  border-radius: 0.375rem;
  background: transparent;
  font-size: 0.8125rem;
  font-weight: 600;
  color: #fff;
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease;
  letter-spacing: -0.0094em;
}
.styles-module__dropdownButton___16NPz:hover {
  background: rgba(255, 255, 255, 0.08);
}
.styles-module__dropdownButton___16NPz svg {
  opacity: 0.6;
}

.styles-module__cycleButton___FMKfw {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0;
  border: none;
  background: transparent;
  font-size: 0.8125rem;
  font-weight: 500;
  color: #fff;
  cursor: pointer;
  letter-spacing: -0.0094em;
}
[data-agentation-theme=light] .styles-module__cycleButton___FMKfw {
  color: rgba(0, 0, 0, 0.85);
}
.styles-module__cycleButton___FMKfw:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.styles-module__settingsRowDisabled___EgS0V .styles-module__settingsLabel___8UjfX {
  color: rgba(255, 255, 255, 0.2);
}
[data-agentation-theme=light] .styles-module__settingsRowDisabled___EgS0V .styles-module__settingsLabel___8UjfX {
  color: rgba(0, 0, 0, 0.2);
}
.styles-module__settingsRowDisabled___EgS0V .styles-module__toggleSwitch___l4Ygm {
  opacity: 0.4;
  cursor: not-allowed;
}

@keyframes styles-module__cycleTextIn___Q6zJf {
  0% {
    opacity: 0;
    transform: translateY(-6px);
  }
  100% {
    opacity: 1;
    transform: translateY(0);
  }
}
.styles-module__cycleButtonText___fD1LR {
  display: inline-block;
  animation: styles-module__cycleTextIn___Q6zJf 0.2s ease-out;
}

.styles-module__cycleDots___LWuoQ {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.styles-module__cycleDot___nPgLY {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.3);
  transform: scale(0.667);
  transition: background-color 0.25s ease-out, transform 0.25s ease-out;
}
.styles-module__cycleDot___nPgLY.styles-module__active___-zoN6 {
  background: #fff;
  transform: scale(1);
}
[data-agentation-theme=light] .styles-module__cycleDot___nPgLY {
  background: rgba(0, 0, 0, 0.2);
}
[data-agentation-theme=light] .styles-module__cycleDot___nPgLY.styles-module__active___-zoN6 {
  background: rgba(0, 0, 0, 0.7);
}

.styles-module__dropdownMenu___k73ER {
  position: absolute;
  right: 0;
  top: calc(100% + 0.25rem);
  background: #1a1a1a;
  border-radius: 0.5rem;
  padding: 0.25rem;
  min-width: 120px;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3), 0 0 0 1px rgba(255, 255, 255, 0.1);
  z-index: 10;
  animation: styles-module__scaleIn___c-r1K 0.15s ease-out;
}

.styles-module__dropdownItem___ylsLj {
  width: 100%;
  display: flex;
  align-items: center;
  padding: 0.5rem 0.625rem;
  border: none;
  border-radius: 0.375rem;
  background: transparent;
  font-size: 0.8125rem;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.85);
  cursor: pointer;
  text-align: left;
  transition: background-color 0.15s ease, color 0.15s ease;
  letter-spacing: -0.0094em;
}
.styles-module__dropdownItem___ylsLj:hover {
  background: rgba(255, 255, 255, 0.08);
}
.styles-module__dropdownItem___ylsLj.styles-module__selected___OwRqP {
  background: rgba(255, 255, 255, 0.12);
  color: #fff;
  font-weight: 600;
}

.styles-module__settingsLabel___8UjfX {
  font-size: 0.8125rem;
  font-weight: 400;
  letter-spacing: -0.0094em;
  color: rgba(255, 255, 255, 0.5);
  display: flex;
  align-items: center;
  gap: 0.125rem;
}
[data-agentation-theme=light] .styles-module__settingsLabel___8UjfX {
  color: rgba(0, 0, 0, 0.5);
}

.styles-module__settingsLabelMarker___ewdtV {
  padding-top: 3px;
  margin-bottom: 10px;
}

.styles-module__settingsOptions___LyrBA {
  display: flex;
  gap: 0.25rem;
}

.styles-module__settingsOption___UNa12 {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.25rem;
  padding: 0.375rem 0.5rem;
  border: none;
  border-radius: 0.375rem;
  background: transparent;
  font-size: 0.6875rem;
  font-weight: 500;
  color: rgba(0, 0, 0, 0.7);
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease;
}
.styles-module__settingsOption___UNa12:hover {
  background: rgba(0, 0, 0, 0.05);
}
.styles-module__settingsOption___UNa12.styles-module__selected___OwRqP {
  background: color-mix(in srgb, var(--agentation-color-blue) 15%, transparent);
  color: var(--agentation-color-blue);
}

.styles-module__sliderContainer___ducXj {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.styles-module__slider___GLdxp {
  -webkit-appearance: none;
  appearance: none;
  width: 100%;
  height: 4px;
  background: rgba(255, 255, 255, 0.15);
  border-radius: 2px;
  outline: none;
  cursor: pointer;
}
.styles-module__slider___GLdxp::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 14px;
  height: 14px;
  background: white;
  border-radius: 50%;
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.15s ease;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}
.styles-module__slider___GLdxp::-moz-range-thumb {
  width: 14px;
  height: 14px;
  background: white;
  border: none;
  border-radius: 50%;
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.15s ease;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}
.styles-module__slider___GLdxp:hover::-webkit-slider-thumb {
  transform: scale(1.15);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
}
.styles-module__slider___GLdxp:hover::-moz-range-thumb {
  transform: scale(1.15);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
}

.styles-module__sliderLabels___FhLDB {
  display: flex;
  justify-content: space-between;
}

.styles-module__sliderLabel___U8sPr {
  font-size: 0.625rem;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.4);
  cursor: pointer;
  transition: color 0.15s ease;
}
.styles-module__sliderLabel___U8sPr:hover {
  color: rgba(255, 255, 255, 0.7);
}
.styles-module__sliderLabel___U8sPr.styles-module__active___-zoN6 {
  color: rgba(255, 255, 255, 0.9);
}

.styles-module__colorOptions___iHCNX {
  display: flex;
  gap: 0.5rem;
  margin-top: 0.375rem;
  margin-bottom: 1px;
}

.styles-module__colorOption___IodiY {
  display: block;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid transparent;
  background-color: var(--swatch);
  cursor: pointer;
  transition: transform 0.2s cubic-bezier(0.25, 1, 0.5, 1);
}
@supports (color: color(display-p3 0 0 0)) {
  .styles-module__colorOption___IodiY {
    background-color: var(--swatch-p3);
  }
}
.styles-module__colorOption___IodiY:hover {
  transform: scale(1.15);
}
.styles-module__colorOption___IodiY.styles-module__selected___OwRqP {
  transform: scale(0.83);
}

.styles-module__colorOptionRing___U2xpo {
  display: flex;
  width: 24px;
  height: 24px;
  border: 2px solid transparent;
  border-radius: 50%;
  transition: border-color 0.3s ease;
}
.styles-module__colorOptionRing___U2xpo.styles-module__selected___OwRqP {
  border-color: var(--swatch);
}
@supports (color: color(display-p3 0 0 0)) {
  .styles-module__colorOptionRing___U2xpo.styles-module__selected___OwRqP {
    border-color: var(--swatch-p3);
  }
}

.styles-module__settingsToggle___fBrFn {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  cursor: pointer;
}
.styles-module__settingsToggle___fBrFn + .styles-module__settingsToggle___fBrFn {
  margin-top: calc(0.5rem + 6px);
}
.styles-module__settingsToggle___fBrFn input[type=checkbox] {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}
.styles-module__settingsToggle___fBrFn.styles-module__settingsToggleMarginBottom___MZUyF {
  margin-bottom: calc(0.5rem + 6px);
}

@keyframes styles-module__mcpPulse___uNggr {
  0% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-green) 50%, transparent);
  }
  70% {
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--agentation-color-green) 0%, transparent);
  }
  100% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-green) 0%, transparent);
  }
}
@keyframes styles-module__mcpPulseError___fov9B {
  0% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-red) 50%, transparent);
  }
  70% {
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--agentation-color-red) 0%, transparent);
  }
  100% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-red) 0%, transparent);
  }
}
.styles-module__mcpStatusDot___ibgkc {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.styles-module__mcpStatusDot___ibgkc.styles-module__connecting___uo-CW {
  background-color: var(--agentation-color-yellow);
  animation: styles-module__mcpPulse___uNggr 1.5s infinite;
}
.styles-module__mcpStatusDot___ibgkc.styles-module__connected___7c28g {
  background-color: var(--agentation-color-green);
  animation: styles-module__mcpPulse___uNggr 2.5s ease-in-out infinite;
}
.styles-module__mcpStatusDot___ibgkc.styles-module__disconnected___cHPxR {
  background-color: var(--agentation-color-red);
  animation: styles-module__mcpPulseError___fov9B 2s infinite;
}

.styles-module__drawCanvas___7cG9U {
  position: fixed;
  inset: 0;
  z-index: 99996;
  pointer-events: none !important;
}
.styles-module__drawCanvas___7cG9U.styles-module__active___-zoN6 {
  pointer-events: auto !important;
  cursor: crosshair !important;
}
.styles-module__drawCanvas___7cG9U.styles-module__active___-zoN6[data-stroke-hover] {
  cursor: pointer !important;
}

.styles-module__dragSelection___kZLq2 {
  position: fixed;
  top: 0;
  left: 0;
  border: 2px solid color-mix(in srgb, var(--agentation-color-green) 60%, transparent);
  border-radius: 4px;
  background-color: color-mix(in srgb, var(--agentation-color-green) 8%, transparent);
  pointer-events: none;
  z-index: 99997;
  will-change: transform, width, height;
  contain: layout style;
}

.styles-module__dragCount___KM90j {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background-color: var(--agentation-color-green);
  color: white;
  font-size: 0.875rem;
  font-weight: 600;
  padding: 0.25rem 0.5rem;
  border-radius: 1rem;
  min-width: 1.5rem;
  text-align: center;
}

.styles-module__highlightsContainer___-0xzG {
  position: fixed;
  top: 0;
  left: 0;
  pointer-events: none;
  z-index: 99996;
}

.styles-module__selectedElementHighlight___fyVlI {
  position: fixed;
  top: 0;
  left: 0;
  border: 2px solid color-mix(in srgb, var(--agentation-color-green) 50%, transparent);
  border-radius: 4px;
  background: color-mix(in srgb, var(--agentation-color-green) 6%, transparent);
  pointer-events: none;
  will-change: transform, width, height;
  contain: layout style;
}

[data-agentation-theme=light] .styles-module__toolbarContainer___dIhma {
  background: #fff;
  color: rgba(0, 0, 0, 0.85);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08), 0 4px 16px rgba(0, 0, 0, 0.06), 0 0 0 1px rgba(0, 0, 0, 0.04);
}
[data-agentation-theme=light] .styles-module__toolbarContainer___dIhma.styles-module__collapsed___Rydsn:hover {
  background: #f5f5f5;
}
[data-agentation-theme=light] .styles-module__toggleContent___0yfyP {
  color: rgba(0, 0, 0, 0.85);
}
[data-agentation-theme=light] .styles-module__toggleContent___0yfyP.styles-module__expandedToggle___F7SRN {
  color: rgba(0, 0, 0, 0.5);
}
[data-agentation-theme=light] .styles-module__toggleContent___0yfyP.styles-module__expandedToggle___F7SRN:hover {
  color: rgba(0, 0, 0, 0.85);
}
[data-agentation-theme=light] .styles-module__toggleContent___0yfyP.styles-module__expandedToggle___F7SRN:hover::before {
  background: rgba(0, 0, 0, 0.06);
}
[data-agentation-theme=light] .styles-module__controlButton___8Q0jc {
  color: rgba(0, 0, 0, 0.5);
}
[data-agentation-theme=light] .styles-module__controlButton___8Q0jc:hover:not(:disabled):not([data-active=true]):not([data-failed=true]):not([data-auto-sync=true]):not([data-error=true]):not([data-no-hover=true]) {
  background: rgba(0, 0, 0, 0.06);
  color: rgba(0, 0, 0, 0.85);
}
[data-agentation-theme=light] .styles-module__controlButton___8Q0jc[data-active=true] {
  color: var(--agentation-color-blue);
  background: color-mix(in srgb, var(--agentation-color-blue) 15%, transparent);
}
[data-agentation-theme=light] .styles-module__controlButton___8Q0jc[data-error=true] {
  color: var(--agentation-color-red);
  background: color-mix(in srgb, var(--agentation-color-red) 15%, transparent);
}
[data-agentation-theme=light] .styles-module__controlButton___8Q0jc[data-danger]:hover:not(:disabled):not([data-active=true]):not([data-failed=true]) {
  color: var(--agentation-color-red);
  background: color-mix(in srgb, var(--agentation-color-red) 15%, transparent);
}
[data-agentation-theme=light] .styles-module__controlButton___8Q0jc[data-auto-sync=true] {
  color: var(--agentation-color-green);
  background: transparent;
}
[data-agentation-theme=light] .styles-module__controlButton___8Q0jc[data-failed=true] {
  color: var(--agentation-color-red);
  background: color-mix(in srgb, var(--agentation-color-red) 15%, transparent);
}
[data-agentation-theme=light] .styles-module__buttonTooltip___Burd9 {
  background: #fff;
  color: rgba(0, 0, 0, 0.85);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08), 0 4px 16px rgba(0, 0, 0, 0.06), 0 0 0 1px rgba(0, 0, 0, 0.04);
}
[data-agentation-theme=light] .styles-module__buttonTooltip___Burd9::after {
  background: #fff;
}
[data-agentation-theme=light] .styles-module__divider___c--s1 {
  background: rgba(0, 0, 0, 0.1);
}`,z={toolbar:"styles-module__toolbar___wNsdK",markersLayer:"styles-module__markersLayer___-25j1",fixedMarkersLayer:"styles-module__fixedMarkersLayer___ffyX6",controlsContent:"styles-module__controlsContent___9GJWU",disableTransitions:"styles-module__disableTransitions___EopxO",positionContext:"styles-module__positionContext___AZFHE",toolbarContainer:"styles-module__toolbarContainer___dIhma",entrance:"styles-module__entrance___sgHd8",toolbarEnter:"styles-module__toolbarEnter___u8RRu",hiding:"styles-module__hiding___1td44",toolbarHide:"styles-module__toolbarHide___y8kaT",collapsed:"styles-module__collapsed___Rydsn",expanded:"styles-module__expanded___ofKPx",serverConnected:"styles-module__serverConnected___Gfbou",buttonWrapper:"styles-module__buttonWrapper___rBcdv",toggleWrapper:"styles-module__toggleWrapper___7N0-q",togglePlaceholder:"styles-module__togglePlaceholder___wnqrL",toggleContent:"styles-module__toggleContent___0yfyP",expandedToggle:"styles-module__expandedToggle___F7SRN",toggleGlyph:"styles-module__toggleGlyph___R7Oom",toggleIcon:"styles-module__toggleIcon___Jbtus",toggleTopLine:"styles-module__toggleTopLine___hQaCm",toggleMiddleLine:"styles-module__toggleMiddleLine___sFFVe",toggleBottomLine:"styles-module__toggleBottomLine___V-jX3",toggleSparkle:"styles-module__toggleSparkle___eeF99",controlButton:"styles-module__controlButton___8Q0jc",visible:"styles-module__visible___KHwEW",hidden:"styles-module__hidden___Ae8H4",badge:"styles-module__badge___2XsgF",fadeOut:"styles-module__fadeOut___6Ut6-",badgeEnter:"styles-module__badgeEnter___mVQLj",statusShowing:"styles-module__statusShowing___te6iu",buttonBadge:"styles-module__buttonBadge___NeFWb",mcpIndicator:"styles-module__mcpIndicator___zGJeL",connected:"styles-module__connected___7c28g",mcpIndicatorPulseConnected:"styles-module__mcpIndicatorPulseConnected___EDodZ",connecting:"styles-module__connecting___uo-CW",mcpIndicatorPulseConnecting:"styles-module__mcpIndicatorPulseConnecting___cCYte",connectionIndicatorWrapper:"styles-module__connectionIndicatorWrapper___L-e-3",connectionIndicator:"styles-module__connectionIndicator___afk9p",connectionIndicatorVisible:"styles-module__connectionIndicatorVisible___C-i5B",connectionIndicatorConnected:"styles-module__connectionIndicatorConnected___IY8pR",connectionPulse:"styles-module__connectionPulse___-Zycw",connectionIndicatorDisconnected:"styles-module__connectionIndicatorDisconnected___kmpaZ",connectionIndicatorConnecting:"styles-module__connectionIndicatorConnecting___QmSLH",buttonTooltip:"styles-module__buttonTooltip___Burd9",tooltipsInSession:"styles-module__tooltipsInSession___-0lHH",sendButtonWrapper:"styles-module__sendButtonWrapper___UUxG6",sendButtonVisible:"styles-module__sendButtonVisible___WPSQU",shortcut:"styles-module__shortcut___lEAQk",tooltipBelow:"styles-module__tooltipBelow___m6ats",tooltipsHidden:"styles-module__tooltipsHidden___VtLJG",tooltipVisible:"styles-module__tooltipVisible___0jcCv",buttonWrapperAlignLeft:"styles-module__buttonWrapperAlignLeft___myzIp",buttonWrapperAlignRight:"styles-module__buttonWrapperAlignRight___HCQFR",divider:"styles-module__divider___c--s1",overlay:"styles-module__overlay___Q1O9y",hoverHighlight:"styles-module__hoverHighlight___ogakW",enter:"styles-module__enter___WFIki",hoverHighlightIn:"styles-module__hoverHighlightIn___6WYHY",multiSelectOutline:"styles-module__multiSelectOutline___cSJ-m",fadeIn:"styles-module__fadeIn___b9qmf",exit:"styles-module__exit___fyOJ0",singleSelectOutline:"styles-module__singleSelectOutline___QhX-O",hoverTooltip:"styles-module__hoverTooltip___bvLk7",hoverTooltipIn:"styles-module__hoverTooltipIn___FYGQx",hoverReactPath:"styles-module__hoverReactPath___gx1IJ",hoverElementName:"styles-module__hoverElementName___QMLMl",marker:"styles-module__marker___6sQrs",clearing:"styles-module__clearing___FQ--7",markerIn:"styles-module__markerIn___5FaAP",markerOut:"styles-module__markerOut___GU5jX",pending:"styles-module__pending___2IHLC",fixed:"styles-module__fixed___dBMHC",multiSelect:"styles-module__multiSelect___YWiuz",hovered:"styles-module__hovered___ZgXIy",renumber:"styles-module__renumber___nCTxD",renumberRoll:"styles-module__renumberRoll___Wgbq3",markerTooltip:"styles-module__markerTooltip___aLJID",tooltipIn:"styles-module__tooltipIn___0N31w",markerQuote:"styles-module__markerQuote___FHmrz",markerNote:"styles-module__markerNote___QkrrS",markerHint:"styles-module__markerHint___2iF-6",settingsPanel:"styles-module__settingsPanel___OxX3Y",settingsHeader:"styles-module__settingsHeader___pwDY9",settingsBrand:"styles-module__settingsBrand___0gJeM",settingsBrandSlash:"styles-module__settingsBrandSlash___uTG18",settingsVersion:"styles-module__settingsVersion___TUcFq",settingsSection:"styles-module__settingsSection___m-YM2",settingsLabel:"styles-module__settingsLabel___8UjfX",cycleButton:"styles-module__cycleButton___FMKfw",cycleDot:"styles-module__cycleDot___nPgLY",dropdownButton:"styles-module__dropdownButton___16NPz",toggleLabel:"styles-module__toggleLabel___Xm8Aa",customCheckbox:"styles-module__customCheckbox___U39ax",sliderLabel:"styles-module__sliderLabel___U8sPr",slider:"styles-module__slider___GLdxp",themeToggle:"styles-module__themeToggle___2rUjA",settingsOption:"styles-module__settingsOption___UNa12",selected:"styles-module__selected___OwRqP",settingsPanelContainer:"styles-module__settingsPanelContainer___Xksv8",settingsPage:"styles-module__settingsPage___6YfHH",slideLeft:"styles-module__slideLeft___Ps01J",automationsPage:"styles-module__automationsPage___uvCq6",slideIn:"styles-module__slideIn___4-qXe",settingsNavLink:"styles-module__settingsNavLink___wCzJt",settingsNavLinkRight:"styles-module__settingsNavLinkRight___ZWwhj",mcpNavIndicator:"styles-module__mcpNavIndicator___cl9pO",mcpPulse:"styles-module__mcpPulse___uNggr",settingsBackButton:"styles-module__settingsBackButton___bIe2j",automationHeader:"styles-module__automationHeader___InP0r",automationDescription:"styles-module__automationDescription___NKlmo",learnMoreLink:"styles-module__learnMoreLink___8xv-x",autoSendRow:"styles-module__autoSendRow___UblX5",autoSendLabel:"styles-module__autoSendLabel___icDc2",active:"styles-module__active___-zoN6",webhookUrlInput:"styles-module__webhookUrlInput___2375C",settingsSectionExtraPadding:"styles-module__settingsSectionExtraPadding___jdhFV",settingsSectionGrow:"styles-module__settingsSectionGrow___h-5HZ",settingsRow:"styles-module__settingsRow___3sdhc",settingsRowMarginTop:"styles-module__settingsRowMarginTop___zA0Sp",dropdownContainer:"styles-module__dropdownContainer___BVnxe",settingsRowDisabled:"styles-module__settingsRowDisabled___EgS0V",toggleSwitch:"styles-module__toggleSwitch___l4Ygm",cycleButtonText:"styles-module__cycleButtonText___fD1LR",cycleTextIn:"styles-module__cycleTextIn___Q6zJf",cycleDots:"styles-module__cycleDots___LWuoQ",dropdownMenu:"styles-module__dropdownMenu___k73ER",scaleIn:"styles-module__scaleIn___c-r1K",dropdownItem:"styles-module__dropdownItem___ylsLj",settingsLabelMarker:"styles-module__settingsLabelMarker___ewdtV",settingsOptions:"styles-module__settingsOptions___LyrBA",sliderContainer:"styles-module__sliderContainer___ducXj",sliderLabels:"styles-module__sliderLabels___FhLDB",colorOptions:"styles-module__colorOptions___iHCNX",colorOption:"styles-module__colorOption___IodiY",colorOptionRing:"styles-module__colorOptionRing___U2xpo",settingsToggle:"styles-module__settingsToggle___fBrFn",settingsToggleMarginBottom:"styles-module__settingsToggleMarginBottom___MZUyF",mcpStatusDot:"styles-module__mcpStatusDot___ibgkc",disconnected:"styles-module__disconnected___cHPxR",mcpPulseError:"styles-module__mcpPulseError___fov9B",drawCanvas:"styles-module__drawCanvas___7cG9U",dragSelection:"styles-module__dragSelection___kZLq2",dragCount:"styles-module__dragCount___KM90j",highlightsContainer:"styles-module__highlightsContainer___-0xzG",selectedElementHighlight:"styles-module__selectedElementHighlight___fyVlI",scaleOut:"styles-module__scaleOut___Wctwz",slideUp:"styles-module__slideUp___kgD36",slideDown:"styles-module__slideDown___zcdje"};var si=ce(ze());function my({active:e}){return(0,si.jsxs)("svg",{className:z.toggleGlyph,"data-active":e,width:"24",height:"24",viewBox:"0 0 24 24",fill:"none",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round","aria-hidden":"true",children:[(0,si.jsx)("path",{className:z.toggleTopLine,d:"M5.5 6.75H18.5"}),(0,si.jsx)("path",{className:z.toggleMiddleLine,d:"M5.5 12H11.5"}),(0,si.jsx)("path",{className:z.toggleBottomLine,d:"M5.5 17.25H9.25"}),(0,si.jsx)("path",{className:z.toggleSparkle,d:"M16 12.75L16.5179 13.9677C16.8078 14.6494 17.3506 15.1922 18.0323 15.4821L19.25 16L18.0323 16.5179C17.3506 16.8078 16.8078 17.3506 16.5179 18.0323L16 19.25L15.4821 18.0323C15.1922 17.3506 14.6494 16.8078 13.9677 16.5179L12.75 16L13.9677 15.4821C14.6494 15.1922 15.1922 14.6494 15.4821 13.9677L16 12.75Z"})]})}var ot=ce(vt());var te={navigation:{width:800,height:56},hero:{width:800,height:320},header:{width:800,height:80},section:{width:800,height:400},sidebar:{width:240,height:400},footer:{width:800,height:160},modal:{width:480,height:300},card:{width:280,height:240},text:{width:400,height:120},image:{width:320,height:200},video:{width:480,height:270},table:{width:560,height:220},grid:{width:600,height:300},list:{width:300,height:180},chart:{width:400,height:240},button:{width:140,height:40},input:{width:280,height:56},form:{width:360,height:320},tabs:{width:480,height:240},dropdown:{width:200,height:200},toggle:{width:44,height:24},search:{width:320,height:44},avatar:{width:48,height:48},badge:{width:80,height:28},breadcrumb:{width:300,height:24},pagination:{width:300,height:36},progress:{width:240,height:8},divider:{width:600,height:1},accordion:{width:400,height:200},carousel:{width:600,height:300},toast:{width:320,height:64},tooltip:{width:180,height:40},pricing:{width:300,height:360},testimonial:{width:360,height:200},cta:{width:600,height:160},alert:{width:400,height:56},banner:{width:800,height:48},stat:{width:200,height:120},stepper:{width:480,height:48},tag:{width:72,height:28},rating:{width:160,height:28},map:{width:480,height:300},timeline:{width:360,height:320},fileUpload:{width:360,height:180},codeBlock:{width:480,height:200},calendar:{width:300,height:300},notification:{width:360,height:72},productCard:{width:280,height:360},profile:{width:280,height:200},drawer:{width:320,height:400},popover:{width:240,height:160},logo:{width:120,height:40},faq:{width:560,height:320},gallery:{width:560,height:360},checkbox:{width:20,height:20},radio:{width:20,height:20},slider:{width:240,height:32},datePicker:{width:300,height:320},skeleton:{width:320,height:120},chip:{width:96,height:32},icon:{width:24,height:24},spinner:{width:32,height:32},feature:{width:360,height:200},team:{width:560,height:280},login:{width:360,height:360},contact:{width:400,height:320}},hm=[{section:"Layout",items:[{type:"navigation",label:"Navigation",...te.navigation},{type:"header",label:"Header",...te.header},{type:"hero",label:"Hero",...te.hero},{type:"section",label:"Section",...te.section},{type:"sidebar",label:"Sidebar",...te.sidebar},{type:"footer",label:"Footer",...te.footer},{type:"modal",label:"Modal",...te.modal},{type:"banner",label:"Banner",...te.banner},{type:"drawer",label:"Drawer",...te.drawer},{type:"popover",label:"Popover",...te.popover},{type:"divider",label:"Divider",...te.divider}]},{section:"Content",items:[{type:"card",label:"Card",...te.card},{type:"text",label:"Text",...te.text},{type:"image",label:"Image",...te.image},{type:"video",label:"Video",...te.video},{type:"table",label:"Table",...te.table},{type:"grid",label:"Grid",...te.grid},{type:"list",label:"List",...te.list},{type:"chart",label:"Chart",...te.chart},{type:"codeBlock",label:"Code Block",...te.codeBlock},{type:"map",label:"Map",...te.map},{type:"timeline",label:"Timeline",...te.timeline},{type:"calendar",label:"Calendar",...te.calendar},{type:"accordion",label:"Accordion",...te.accordion},{type:"carousel",label:"Carousel",...te.carousel},{type:"logo",label:"Logo",...te.logo},{type:"faq",label:"FAQ",...te.faq},{type:"gallery",label:"Gallery",...te.gallery}]},{section:"Controls",items:[{type:"button",label:"Button",...te.button},{type:"input",label:"Input",...te.input},{type:"search",label:"Search",...te.search},{type:"form",label:"Form",...te.form},{type:"tabs",label:"Tabs",...te.tabs},{type:"dropdown",label:"Dropdown",...te.dropdown},{type:"toggle",label:"Toggle",...te.toggle},{type:"stepper",label:"Stepper",...te.stepper},{type:"rating",label:"Rating",...te.rating},{type:"fileUpload",label:"File Upload",...te.fileUpload},{type:"checkbox",label:"Checkbox",...te.checkbox},{type:"radio",label:"Radio",...te.radio},{type:"slider",label:"Slider",...te.slider},{type:"datePicker",label:"Date Picker",...te.datePicker}]},{section:"Elements",items:[{type:"avatar",label:"Avatar",...te.avatar},{type:"badge",label:"Badge",...te.badge},{type:"tag",label:"Tag",...te.tag},{type:"breadcrumb",label:"Breadcrumb",...te.breadcrumb},{type:"pagination",label:"Pagination",...te.pagination},{type:"progress",label:"Progress",...te.progress},{type:"alert",label:"Alert",...te.alert},{type:"toast",label:"Toast",...te.toast},{type:"notification",label:"Notification",...te.notification},{type:"tooltip",label:"Tooltip",...te.tooltip},{type:"stat",label:"Stat",...te.stat},{type:"skeleton",label:"Skeleton",...te.skeleton},{type:"chip",label:"Chip",...te.chip},{type:"icon",label:"Icon",...te.icon},{type:"spinner",label:"Spinner",...te.spinner}]},{section:"Blocks",items:[{type:"pricing",label:"Pricing",...te.pricing},{type:"testimonial",label:"Testimonial",...te.testimonial},{type:"cta",label:"CTA",...te.cta},{type:"productCard",label:"Product Card",...te.productCard},{type:"profile",label:"Profile",...te.profile},{type:"feature",label:"Feature",...te.feature},{type:"team",label:"Team",...te.team},{type:"login",label:"Login",...te.login},{type:"contact",label:"Contact",...te.contact}]}],Fn={};for(let e of hm)for(let t of e.items)Fn[t.type]=t;var _=ce(ze());function H({w:e,h:t=3,strong:n}){return(0,_.jsx)("div",{style:{width:typeof e=="number"?`${e}px`:e,height:t,borderRadius:2,background:n?"var(--agd-bar-strong)":"var(--agd-bar)",flexShrink:0}})}function ft({w:e,h:t,radius:n=3,style:o}){return(0,_.jsx)("div",{style:{width:typeof e=="number"?`${e}px`:e,height:typeof t=="number"?`${t}px`:t,borderRadius:n,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",flexShrink:0,...o}})}function xn({size:e}){return(0,_.jsx)("div",{style:{width:e,height:e,borderRadius:"50%",border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",flexShrink:0}})}function _v({width:e,height:t}){let n=Math.max(8,t*.2);return(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",height:"100%",padding:`0 ${n}px`,gap:e*.02},children:[(0,_.jsx)(ft,{w:Math.max(20,t*.5),h:Math.max(12,t*.4),radius:2}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",gap:e*.03,marginLeft:e*.04},children:[(0,_.jsx)(H,{w:e*.06}),(0,_.jsx)(H,{w:e*.07}),(0,_.jsx)(H,{w:e*.05}),(0,_.jsx)(H,{w:e*.06})]}),(0,_.jsx)(ft,{w:e*.1,h:Math.min(28,t*.5),radius:4})]})}function mv({width:e,height:t,text:n}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",height:"100%",gap:t*.05},children:[n?(0,_.jsx)("span",{style:{fontSize:Math.min(20,t*.08),fontWeight:600,color:"var(--agd-text-3)",textAlign:"center",maxWidth:"80%"},children:n}):(0,_.jsx)(H,{w:e*.5,h:Math.max(6,t*.04),strong:!0}),(0,_.jsx)(H,{w:e*.6}),(0,_.jsx)(H,{w:e*.4}),(0,_.jsx)(ft,{w:Math.min(140,e*.2),h:Math.min(36,t*.12),radius:6,style:{marginTop:t*.06}})]})}function pv({width:e,height:t}){let n=Math.max(3,Math.floor(t/36));return(0,_.jsxs)("div",{style:{padding:e*.08,display:"flex",flexDirection:"column",gap:t*.03},children:[(0,_.jsx)(H,{w:e*.6,h:4,strong:!0}),Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:6},children:[(0,_.jsx)(ft,{w:10,h:10,radius:2}),(0,_.jsx)(H,{w:e*(.4+r*17%30/100)})]},r))]})}function fv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/160)));return(0,_.jsx)("div",{style:{display:"flex",padding:`${t*.12}px ${e*.03}px`,gap:e*.05},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsx)(H,{w:"60%",h:3,strong:!0}),(0,_.jsx)(H,{w:"80%",h:2}),(0,_.jsx)(H,{w:"70%",h:2}),(0,_.jsx)(H,{w:"60%",h:2})]},r))})}function gv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsxs)("div",{style:{padding:"10px 12px",borderBottom:"1px solid var(--agd-stroke)",display:"flex",alignItems:"center",justifyContent:"space-between"},children:[(0,_.jsx)(H,{w:e*.3,h:4,strong:!0}),(0,_.jsx)("div",{style:{width:14,height:14,border:"1px solid var(--agd-stroke)",borderRadius:3}})]}),(0,_.jsxs)("div",{style:{flex:1,padding:12,display:"flex",flexDirection:"column",gap:6},children:[(0,_.jsx)(H,{w:"90%"}),(0,_.jsx)(H,{w:"70%"}),(0,_.jsx)(H,{w:"80%"})]}),(0,_.jsxs)("div",{style:{padding:"10px 12px",borderTop:"1px solid var(--agd-stroke)",display:"flex",justifyContent:"flex-end",gap:8},children:[(0,_.jsx)(ft,{w:70,h:26,radius:4}),(0,_.jsx)(ft,{w:70,h:26,radius:4,style:{background:"var(--agd-bar)"}})]})]})}function hv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{height:"40%",background:"var(--agd-fill)",borderBottom:"1px dashed var(--agd-stroke)"}}),(0,_.jsxs)("div",{style:{flex:1,padding:10,display:"flex",flexDirection:"column",gap:5},children:[(0,_.jsx)(H,{w:"70%",h:4,strong:!0}),(0,_.jsx)(H,{w:"95%",h:2}),(0,_.jsx)(H,{w:"85%",h:2}),(0,_.jsx)(H,{w:"50%",h:2})]})]})}function yv({width:e,height:t,text:n}){if(n)return(0,_.jsx)("div",{style:{padding:4,fontSize:Math.min(14,t*.3),lineHeight:1.5,color:"var(--agd-text-3)",wordBreak:"break-word",overflow:"hidden"},children:n});let o=Math.max(2,Math.floor(t/18));return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:6,padding:4},children:[(0,_.jsx)(H,{w:e*.6,h:5,strong:!0}),Array.from({length:o},(r,i)=>(0,_.jsx)(H,{w:`${70+i*13%25}%`,h:2},i))]})}function xv({width:e,height:t}){return(0,_.jsx)("div",{style:{height:"100%",position:"relative"},children:(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,preserveAspectRatio:"none",fill:"none",children:[(0,_.jsx)("line",{x1:"0",y1:"0",x2:e,y2:t,stroke:"var(--agd-stroke)",strokeWidth:"1"}),(0,_.jsx)("line",{x1:e,y1:"0",x2:"0",y2:t,stroke:"var(--agd-stroke)",strokeWidth:"1"}),(0,_.jsx)("circle",{cx:e*.3,cy:t*.3,r:Math.min(e,t)*.08,fill:"var(--agd-fill)",stroke:"var(--agd-stroke)",strokeWidth:"0.8"})]})})}function vv({width:e,height:t}){let n=Math.max(2,Math.min(5,Math.floor(e/100))),o=Math.max(2,Math.min(6,Math.floor(t/32)));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{display:"flex",borderBottom:"1px solid var(--agd-stroke)",padding:"6px 0"},children:Array.from({length:n},(r,i)=>(0,_.jsx)("div",{style:{flex:1,padding:"0 8px"},children:(0,_.jsx)(H,{w:"70%",h:3,strong:!0})},i))}),Array.from({length:o},(r,i)=>(0,_.jsx)("div",{style:{display:"flex",borderBottom:"1px solid rgba(255,255,255,0.03)",padding:"6px 0"},children:Array.from({length:n},(s,l)=>(0,_.jsx)("div",{style:{flex:1,padding:"0 8px"},children:(0,_.jsx)(H,{w:`${50+(i*7+l*13)%40}%`,h:2})},l))},i))]})}function wv({width:e,height:t}){let n=Math.max(2,Math.floor(t/28));return(0,_.jsx)("div",{style:{display:"flex",flexDirection:"column",gap:4,padding:4},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:8,padding:"4px 0"},children:[(0,_.jsx)(xn,{size:8}),(0,_.jsx)(H,{w:`${55+r*17%35}%`,h:2})]},r))})}function bv({width:e,height:t,text:n}){return(0,_.jsx)("div",{style:{height:"100%",borderRadius:Math.min(8,t/3),border:"1px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center"},children:n?(0,_.jsx)("span",{style:{fontSize:Math.min(13,t*.4),fontWeight:500,color:"var(--agd-text-3)",letterSpacing:"-0.01em"},children:n}):(0,_.jsx)(H,{w:Math.max(20,e*.5),h:3,strong:!0})})}function kv({width:e,height:t}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:4,height:"100%",justifyContent:"center"},children:[(0,_.jsx)(H,{w:Math.min(80,e*.3),h:2}),(0,_.jsx)("div",{style:{height:Math.min(36,t*.6),borderRadius:4,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",paddingLeft:8},children:(0,_.jsx)(H,{w:"40%",h:2})})]})}function Sv({width:e,height:t}){let n=Math.max(2,Math.min(5,Math.floor(t/56)));return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:t*.04,padding:8},children:[Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsx)(H,{w:60+r*17%30,h:2}),(0,_.jsx)(ft,{w:"100%",h:28,radius:4})]},r)),(0,_.jsx)(ft,{w:Math.min(120,e*.35),h:30,radius:6,style:{marginTop:8,alignSelf:"flex-end",background:"var(--agd-bar)"}})]})}function Cv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/120)));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{display:"flex",gap:2,borderBottom:"1px solid var(--agd-stroke)"},children:Array.from({length:n},(o,r)=>(0,_.jsx)("div",{style:{padding:"8px 12px",borderBottom:r===0?"2px solid var(--agd-bar-strong)":"none"},children:(0,_.jsx)(H,{w:60,h:3,strong:r===0})},r))}),(0,_.jsxs)("div",{style:{flex:1,padding:12,display:"flex",flexDirection:"column",gap:6},children:[(0,_.jsx)(H,{w:"80%",h:2}),(0,_.jsx)(H,{w:"65%",h:2}),(0,_.jsx)(H,{w:"75%",h:2})]})]})}function Ev({width:e,height:t}){let n=Math.min(e,t)/2;return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("circle",{cx:e/2,cy:t/2,r:n-1,stroke:"var(--agd-stroke)",fill:"var(--agd-fill)",strokeWidth:"1.5",strokeDasharray:"3 2"}),(0,_.jsx)("circle",{cx:e/2,cy:t*.38,r:n*.28,stroke:"var(--agd-stroke)",fill:"var(--agd-fill)",strokeWidth:"0.8"}),(0,_.jsx)("path",{d:`M${e/2-n*.55} ${t*.78} C${e/2-n*.55} ${t*.55} ${e/2+n*.55} ${t*.55} ${e/2+n*.55} ${t*.78}`,stroke:"var(--agd-stroke)",fill:"var(--agd-fill)",strokeWidth:"0.8"})]})}function Mv({width:e,height:t}){return(0,_.jsx)("div",{style:{height:"100%",borderRadius:t/2,border:"1px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)(H,{w:Math.max(16,e*.5),h:2,strong:!0})})}function Tv({width:e,height:t}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",height:"100%",gap:t*.08},children:[(0,_.jsx)(H,{w:e*.5,h:Math.max(5,t*.06),strong:!0}),(0,_.jsx)(H,{w:e*.35})]})}function Lv({width:e,height:t}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",height:"100%",gap:t*.04,padding:e*.04},children:[(0,_.jsx)(H,{w:e*.3,h:4,strong:!0}),(0,_.jsx)(H,{w:e*.7}),(0,_.jsx)(H,{w:e*.5}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",gap:e*.03,marginTop:t*.06},children:[(0,_.jsx)(ft,{w:"33%",h:"100%",radius:4}),(0,_.jsx)(ft,{w:"33%",h:"100%",radius:4}),(0,_.jsx)(ft,{w:"33%",h:"100%",radius:4})]})]})}function Rv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/140))),o=Math.max(1,Math.min(3,Math.floor(t/120)));return(0,_.jsx)("div",{style:{display:"grid",gridTemplateColumns:`repeat(${n}, 1fr)`,gridTemplateRows:`repeat(${o}, 1fr)`,gap:6,height:"100%"},children:Array.from({length:n*o},(r,i)=>(0,_.jsx)(ft,{w:"100%",h:"100%",radius:4},i))})}function Pv({width:e,height:t}){let n=Math.max(2,Math.floor((t-32)/28));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{padding:"6px 8px",borderBottom:"1px solid var(--agd-stroke)"},children:(0,_.jsx)(H,{w:e*.5,h:3,strong:!0})}),(0,_.jsx)("div",{style:{flex:1,padding:4,display:"flex",flexDirection:"column",gap:2},children:Array.from({length:n},(o,r)=>(0,_.jsx)("div",{style:{padding:"4px 6px",borderRadius:3,background:r===0?"var(--agd-fill)":"transparent"},children:(0,_.jsx)(H,{w:`${50+r*17%35}%`,h:2,strong:r===0})},r))})]})}function Nv({width:e,height:t}){let n=Math.min(e,t)/2;return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("rect",{x:"1",y:"1",width:e-2,height:t-2,rx:n,stroke:"var(--agd-stroke)",strokeWidth:"1"}),(0,_.jsx)("circle",{cx:e-n,cy:t/2,r:n*.7,fill:"var(--agd-bar)"})]})}function Iv({width:e,height:t}){let n=Math.min(t/2,20);return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:n,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:`0 ${n*.6}px`,gap:6},children:[(0,_.jsx)(xn,{size:Math.min(14,t*.4)}),(0,_.jsx)(H,{w:"50%",h:2})]})}function Dv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:8,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:"0 10px",gap:8},children:[(0,_.jsx)(xn,{size:Math.min(20,t*.5)}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(H,{w:"60%",h:3,strong:!0}),(0,_.jsx)(H,{w:"80%",h:2})]}),(0,_.jsx)("div",{style:{width:14,height:14,border:"1px solid var(--agd-stroke)",borderRadius:3,flexShrink:0}})]})}function $v({width:e,height:t}){return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("rect",{x:"0",y:"0",width:e,height:t,rx:t/2,stroke:"var(--agd-stroke)",strokeWidth:"0.8"}),(0,_.jsx)("rect",{x:"1",y:"1",width:e*.65,height:t-2,rx:(t-2)/2,fill:"var(--agd-bar)"})]})}function Av({width:e,height:t}){let n=Math.max(3,Math.min(7,Math.floor(e/50))),o=e/(n*2);return(0,_.jsx)("div",{style:{height:"100%",display:"flex",alignItems:"flex-end",justifyContent:"space-around",padding:"0 4px",borderBottom:"1px solid var(--agd-stroke)"},children:Array.from({length:n},(r,i)=>{let s=30+(i*37+17)%55;return(0,_.jsx)(ft,{w:o,h:`${s}%`,radius:2},i)})})}function Fv({width:e,height:t}){let n=Math.min(e,t)*.12;return(0,_.jsxs)("div",{style:{height:"100%",position:"relative",display:"flex",alignItems:"center",justifyContent:"center"},children:[(0,_.jsx)(ft,{w:"100%",h:"100%",radius:4}),(0,_.jsx)("div",{style:{position:"absolute",width:n*2,height:n*2,borderRadius:"50%",border:"1.5px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)("div",{style:{width:0,height:0,borderLeft:`${n*.6}px solid var(--agd-bar-strong)`,borderTop:`${n*.4}px solid transparent`,borderBottom:`${n*.4}px solid transparent`,marginLeft:n*.15}})})]})}function Ov({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center"},children:[(0,_.jsx)("div",{style:{flex:1,width:"100%",borderRadius:6,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)(H,{w:"60%",h:2})}),(0,_.jsx)("div",{style:{width:8,height:8,background:"var(--agd-fill)",border:"1px dashed var(--agd-stroke)",borderTop:"none",borderLeft:"none",transform:"rotate(45deg)",marginTop:-5}})]})}function Bv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/80)));return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",height:"100%",gap:4},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:4},children:[r>0&&(0,_.jsx)("span",{style:{color:"var(--agd-stroke)",fontSize:10},children:"/"}),(0,_.jsx)(H,{w:40+r*13%20,h:2,strong:r===n-1})]},r))})}function zv({width:e,height:t}){let n=Math.max(3,Math.min(5,Math.floor(e/40))),o=Math.min(28,t*.8);return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:"100%",gap:4},children:Array.from({length:n},(r,i)=>(0,_.jsx)(ft,{w:o,h:o,radius:4,style:i===1?{background:"var(--agd-bar)"}:void 0},i))})}function Hv({width:e}){return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",height:"100%"},children:(0,_.jsx)("div",{style:{width:"100%",height:1,background:"var(--agd-stroke)"}})})}function Wv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(t/40)));return(0,_.jsx)("div",{style:{display:"flex",flexDirection:"column",height:"100%"},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{borderBottom:"1px solid var(--agd-stroke)",padding:"8px 6px",display:"flex",alignItems:"center",justifyContent:"space-between",flex:r===0?2:1},children:[(0,_.jsx)(H,{w:`${40+r*17%25}%`,h:3,strong:!0}),(0,_.jsx)("span",{style:{fontSize:8,color:"var(--agd-stroke)"},children:r===0?"\u25BC":"\u25B6"})]},r))})}function Uv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",gap:6},children:[(0,_.jsxs)("div",{style:{flex:1,display:"flex",gap:6,alignItems:"center"},children:[(0,_.jsx)("span",{style:{fontSize:12,color:"var(--agd-stroke)"},children:"\u2039"}),(0,_.jsx)(ft,{w:"100%",h:"100%",radius:4}),(0,_.jsx)("span",{style:{fontSize:12,color:"var(--agd-stroke)"},children:"\u203A"})]}),(0,_.jsxs)("div",{style:{display:"flex",justifyContent:"center",gap:4},children:[(0,_.jsx)(xn,{size:5}),(0,_.jsx)(xn,{size:5}),(0,_.jsx)(xn,{size:5})]})]})}function jv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",padding:10,gap:t*.04},children:[(0,_.jsx)(H,{w:e*.4,h:3,strong:!0}),(0,_.jsx)(H,{w:e*.3,h:6,strong:!0}),(0,_.jsx)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:4,width:"100%",padding:"8px 0"},children:Array.from({length:4},(n,o)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:4},children:[(0,_.jsx)(xn,{size:5}),(0,_.jsx)(H,{w:`${50+o*17%35}%`,h:2})]},o))}),(0,_.jsx)(ft,{w:e*.7,h:Math.min(32,t*.1),radius:6,style:{background:"var(--agd-bar)"}})]})}function Yv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",padding:10,gap:8},children:[(0,_.jsx)("span",{style:{fontSize:18,lineHeight:1,color:"var(--agd-stroke)",fontFamily:"serif"},children:"\u201C"}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsx)(H,{w:"90%",h:2}),(0,_.jsx)(H,{w:"75%",h:2}),(0,_.jsx)(H,{w:"60%",h:2})]}),(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:6},children:[(0,_.jsx)(xn,{size:20}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:2},children:[(0,_.jsx)(H,{w:60,h:3,strong:!0}),(0,_.jsx)(H,{w:40,h:2})]})]})]})}function Vv({width:e,height:t}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",height:"100%",gap:t*.08},children:[(0,_.jsx)(H,{w:e*.5,h:Math.max(4,t*.05),strong:!0}),(0,_.jsx)(H,{w:e*.35}),(0,_.jsx)(ft,{w:Math.min(140,e*.25),h:Math.min(32,t*.15),radius:6,style:{marginTop:t*.04,background:"var(--agd-bar)"}})]})}function Xv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:6,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:"0 10px",gap:8},children:[(0,_.jsx)("div",{style:{width:16,height:16,borderRadius:"50%",border:"1.5px solid var(--agd-bar-strong)",display:"flex",alignItems:"center",justifyContent:"center",flexShrink:0},children:(0,_.jsx)("div",{style:{width:2,height:6,background:"var(--agd-bar-strong)",borderRadius:1}})}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(H,{w:"40%",h:3,strong:!0}),(0,_.jsx)(H,{w:"70%",h:2})]})]})}function Qv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center",gap:8,padding:"0 12px"},children:[(0,_.jsx)(H,{w:e*.4,h:3,strong:!0}),(0,_.jsx)(ft,{w:60,h:Math.min(24,t*.6),radius:4})]})}function Gv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",gap:t*.06},children:[(0,_.jsx)(H,{w:e*.5,h:2}),(0,_.jsx)(H,{w:e*.4,h:Math.max(8,t*.18),strong:!0}),(0,_.jsx)(H,{w:e*.3,h:2})]})}function qv({width:e,height:t}){let n=Math.max(3,Math.min(5,Math.floor(e/100))),o=Math.min(12,t*.35);return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"space-between",height:"100%",padding:"0 8px"},children:Array.from({length:n},(r,i)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:0,flex:1},children:[(0,_.jsx)("div",{style:{width:o,height:o,borderRadius:"50%",border:"1.5px solid var(--agd-stroke)",background:i===0?"var(--agd-bar)":"transparent",flexShrink:0}}),i<n-1&&(0,_.jsx)("div",{style:{flex:1,height:1,background:"var(--agd-stroke)",margin:"0 4px"}})]},i))})}function Kv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:4,border:"1px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center",gap:4,padding:"0 6px"},children:[(0,_.jsx)(H,{w:Math.max(16,e*.5),h:2,strong:!0}),(0,_.jsx)("div",{style:{width:8,height:8,borderRadius:"50%",border:"1px solid var(--agd-stroke)",flexShrink:0}})]})}function Jv({width:e,height:t}){let o=Math.min(t*.7,e/7.5);return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:"100%",gap:o*.2},children:Array.from({length:5},(r,i)=>(0,_.jsx)("svg",{width:o,height:o,viewBox:"0 0 16 16",fill:"none",children:(0,_.jsx)("path",{d:"M8 1.5l2 4 4.5.7-3.25 3.1.75 4.5L8 11.4l-4 2.4.75-4.5L1.5 6.2 6 5.5z",stroke:"var(--agd-stroke)",strokeWidth:"0.8",fill:i<3?"var(--agd-bar)":"none"})},i))})}function Zv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",position:"relative",borderRadius:4,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",overflow:"hidden"},children:[(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",style:{position:"absolute",inset:0},children:[(0,_.jsx)("line",{x1:0,y1:t*.3,x2:e,y2:t*.7,stroke:"var(--agd-stroke)",strokeWidth:"0.5",opacity:".2"}),(0,_.jsx)("line",{x1:0,y1:t*.6,x2:e,y2:t*.2,stroke:"var(--agd-stroke)",strokeWidth:"0.5",opacity:".15"}),(0,_.jsx)("line",{x1:e*.4,y1:0,x2:e*.6,y2:t,stroke:"var(--agd-stroke)",strokeWidth:"0.5",opacity:".15"})]}),(0,_.jsx)("div",{style:{position:"absolute",left:"50%",top:"40%",transform:"translate(-50%, -100%)"},children:(0,_.jsxs)("svg",{width:"16",height:"22",viewBox:"0 0 16 22",fill:"none",children:[(0,_.jsx)("path",{d:"M8 0C3.6 0 0 3.6 0 8c0 6 8 14 8 14s8-8 8-14c0-4.4-3.6-8-8-8z",fill:"var(--agd-bar)",opacity:".4"}),(0,_.jsx)("circle",{cx:"8",cy:"8",r:"3",fill:"var(--agd-fill)"})]})})]})}function ew({width:e,height:t}){let n=Math.max(3,Math.min(5,Math.floor(t/60)));return(0,_.jsxs)("div",{style:{display:"flex",height:"100%",padding:"8px 0"},children:[(0,_.jsx)("div",{style:{width:16,display:"flex",flexDirection:"column",alignItems:"center"},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",flex:1},children:[(0,_.jsx)(xn,{size:8}),r<n-1&&(0,_.jsx)("div",{style:{flex:1,width:1,background:"var(--agd-stroke)"}})]},r))}),(0,_.jsx)("div",{style:{flex:1,display:"flex",flexDirection:"column",justifyContent:"space-around",paddingLeft:8},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(H,{w:`${35+r*13%25}%`,h:3,strong:!0}),(0,_.jsx)(H,{w:`${50+r*17%30}%`,h:2})]},r))})]})}function tw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:8,border:"2px dashed var(--agd-stroke)",display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",gap:t*.06},children:[(0,_.jsxs)("svg",{width:"24",height:"24",viewBox:"0 0 24 24",fill:"none",children:[(0,_.jsx)("path",{d:"M12 16V4m0 0l-4 4m4-4l4 4",stroke:"var(--agd-stroke)",strokeWidth:"1.5"}),(0,_.jsx)("path",{d:"M4 17v2a1 1 0 001 1h14a1 1 0 001-1v-2",stroke:"var(--agd-stroke)",strokeWidth:"1.5"})]}),(0,_.jsx)(H,{w:e*.4,h:2}),(0,_.jsx)(H,{w:e*.25,h:2})]})}function nw({width:e,height:t}){let n=Math.max(3,Math.min(8,Math.floor(t/20)));return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:6,background:"var(--agd-fill)",border:"1px solid var(--agd-stroke)",padding:8,display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsxs)("div",{style:{display:"flex",gap:3,marginBottom:4},children:[(0,_.jsx)(xn,{size:6}),(0,_.jsx)(xn,{size:6}),(0,_.jsx)(xn,{size:6})]}),Array.from({length:n},(o,r)=>(0,_.jsx)("div",{style:{display:"flex",gap:6,paddingLeft:r>0&&r<n-1?12:0},children:(0,_.jsx)(H,{w:`${25+r*23%50}%`,h:2,strong:r===0})},r))]})}function ow({width:e,height:t}){let r=Math.min((e-16)/7,(t-40)/6);return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",justifyContent:"space-between",padding:"6px 8px"},children:[(0,_.jsx)("span",{style:{fontSize:8,color:"var(--agd-stroke)"},children:"\u2039"}),(0,_.jsx)(H,{w:e*.3,h:3,strong:!0}),(0,_.jsx)("span",{style:{fontSize:8,color:"var(--agd-stroke)"},children:"\u203A"})]}),(0,_.jsxs)("div",{style:{display:"grid",gridTemplateColumns:"repeat(7, 1fr)",gap:2,padding:"0 4px",flex:1},children:[Array.from({length:7},(i,s)=>(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:r*.6},children:(0,_.jsx)(H,{w:r*.5,h:2})},`h${s}`)),Array.from({length:35},(i,s)=>(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:r},children:(0,_.jsx)("div",{style:{width:r*.6,height:r*.6,borderRadius:"50%",background:s===12?"var(--agd-bar)":"transparent",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)("div",{style:{width:2,height:2,borderRadius:1,background:"var(--agd-bar-strong)",opacity:s===12?1:.3}})})},s))]})]})}function rw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:8,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:"0 10px",gap:8},children:[(0,_.jsx)(xn,{size:Math.min(32,t*.55)}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(H,{w:"50%",h:3,strong:!0}),(0,_.jsx)(H,{w:"75%",h:2})]}),(0,_.jsx)(H,{w:30,h:2})]})}function iw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{height:"50%",background:"var(--agd-fill)",borderBottom:"1px dashed var(--agd-stroke)"}}),(0,_.jsxs)("div",{style:{flex:1,padding:10,display:"flex",flexDirection:"column",gap:5},children:[(0,_.jsx)(H,{w:"65%",h:4,strong:!0}),(0,_.jsx)(H,{w:"40%",h:3}),(0,_.jsx)("div",{style:{flex:1}}),(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",justifyContent:"space-between"},children:[(0,_.jsx)(H,{w:"30%",h:5,strong:!0}),(0,_.jsx)(ft,{w:Math.min(70,e*.3),h:26,radius:4,style:{background:"var(--agd-bar)"}})]})]})]})}function sw({width:e,height:t}){let n=Math.min(48,t*.3);return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",gap:t*.06},children:[(0,_.jsx)(xn,{size:n}),(0,_.jsx)(H,{w:e*.45,h:4,strong:!0}),(0,_.jsx)(H,{w:e*.3,h:2}),(0,_.jsxs)("div",{style:{display:"flex",gap:e*.08,marginTop:t*.04},children:[(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",gap:2},children:[(0,_.jsx)(H,{w:20,h:3,strong:!0}),(0,_.jsx)(H,{w:28,h:2})]}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",gap:2},children:[(0,_.jsx)(H,{w:20,h:3,strong:!0}),(0,_.jsx)(H,{w:28,h:2})]}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",gap:2},children:[(0,_.jsx)(H,{w:20,h:3,strong:!0}),(0,_.jsx)(H,{w:28,h:2})]})]})]})}function lw({width:e,height:t}){let n=Math.max(e*.6,80),o=Math.max(3,Math.floor(t/40));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex"},children:[(0,_.jsx)("div",{style:{width:e-n,background:"var(--agd-fill)",opacity:.3}}),(0,_.jsxs)("div",{style:{flex:1,borderLeft:"1px solid var(--agd-stroke)",display:"flex",flexDirection:"column",padding:e*.04},children:[(0,_.jsxs)("div",{style:{display:"flex",justifyContent:"space-between",alignItems:"center",marginBottom:t*.06},children:[(0,_.jsx)(H,{w:n*.4,h:4,strong:!0}),(0,_.jsx)("div",{style:{width:12,height:12,border:"1px solid var(--agd-stroke)",borderRadius:3}})]}),Array.from({length:o},(r,i)=>(0,_.jsx)("div",{style:{padding:"6px 0"},children:(0,_.jsx)(H,{w:`${50+i*17%35}%`,h:2,strong:i===0})},i))]})]})}function aw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center"},children:[(0,_.jsxs)("div",{style:{flex:1,width:"100%",borderRadius:8,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",padding:10,display:"flex",flexDirection:"column",gap:5},children:[(0,_.jsx)(H,{w:"70%",h:3,strong:!0}),(0,_.jsx)(H,{w:"90%",h:2}),(0,_.jsx)(H,{w:"60%",h:2})]}),(0,_.jsx)("div",{style:{width:10,height:10,background:"var(--agd-fill)",border:"1px dashed var(--agd-stroke)",borderTop:"none",borderLeft:"none",transform:"rotate(45deg)",marginTop:-6}})]})}function cw({width:e,height:t}){let n=Math.min(t*.7,e*.3);return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",alignItems:"center",gap:e*.08},children:[(0,_.jsx)(ft,{w:n,h:n,radius:n*.25}),(0,_.jsx)(H,{w:e*.45,h:Math.max(4,t*.2),strong:!0})]})}function uw({width:e,height:t}){let n=Math.max(2,Math.min(5,Math.floor(t/56)));return(0,_.jsx)("div",{style:{display:"flex",flexDirection:"column",height:"100%"},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{borderBottom:"1px solid var(--agd-stroke)",padding:"8px 6px",display:"flex",alignItems:"center",justifyContent:"space-between",flex:r===0?2:1},children:[(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:6},children:[(0,_.jsx)("span",{style:{fontSize:9,fontWeight:700,color:"var(--agd-stroke)"},children:"Q"}),(0,_.jsx)(H,{w:e*(.3+r*13%25/100),h:3,strong:!0})]}),(0,_.jsx)("span",{style:{fontSize:8,color:"var(--agd-stroke)"},children:r===0?"\u25BC":"\u25B6"})]},r))})}function dw({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/120))),o=Math.max(1,Math.min(3,Math.floor(t/120)));return(0,_.jsx)("div",{style:{display:"grid",gridTemplateColumns:`repeat(${n}, 1fr)`,gridTemplateRows:`repeat(${o}, 1fr)`,gap:4,height:"100%"},children:Array.from({length:n*o},(r,i)=>(0,_.jsx)("div",{style:{borderRadius:4,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",position:"relative",overflow:"hidden"},children:(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:"0 0 100 100",preserveAspectRatio:"none",fill:"none",children:[(0,_.jsx)("line",{x1:"0",y1:"0",x2:"100",y2:"100",stroke:"var(--agd-stroke)",strokeWidth:"0.5"}),(0,_.jsx)("line",{x1:"100",y1:"0",x2:"0",y2:"100",stroke:"var(--agd-stroke)",strokeWidth:"0.5"})]})},i))})}function _w({width:e,height:t}){let n=Math.min(e,t);return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("rect",{x:"1",y:(t-n+2)/2,width:n-2,height:n-2,rx:n*.15,stroke:"var(--agd-stroke)",strokeWidth:"1.5"}),(0,_.jsx)("path",{d:`M${n*.25} ${t/2}l${n*.2} ${n*.2} ${n*.3}-${n*.35}`,stroke:"var(--agd-bar)",strokeWidth:"1.5",fill:"none",strokeLinecap:"round",strokeLinejoin:"round"})]})}function mw({width:e,height:t}){let n=Math.min(e,t)/2-1;return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("circle",{cx:e/2,cy:t/2,r:n,stroke:"var(--agd-stroke)",strokeWidth:"1.5"}),(0,_.jsx)("circle",{cx:e/2,cy:t/2,r:n*.45,fill:"var(--agd-bar)"})]})}function pw({width:e,height:t}){let n=Math.max(2,t*.12),o=Math.min(t*.35,10),r=e*.55;return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",alignItems:"center",position:"relative"},children:[(0,_.jsx)("div",{style:{width:"100%",height:n,borderRadius:n/2,background:"var(--agd-fill)",border:"1px solid var(--agd-stroke)",position:"relative"},children:(0,_.jsx)("div",{style:{width:r,height:"100%",borderRadius:n/2,background:"var(--agd-bar)"}})}),(0,_.jsx)("div",{style:{position:"absolute",left:r-o,width:o*2,height:o*2,borderRadius:"50%",border:"1.5px solid var(--agd-stroke)",background:"var(--agd-fill)"}})]})}function fw({width:e,height:t}){let n=Math.min(36,t*.15),o=7,r=4,i=Math.min((e-16)/o,(t-n-40)/(r+1));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsxs)("div",{style:{height:n,borderRadius:4,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:"0 8px",justifyContent:"space-between"},children:[(0,_.jsx)(H,{w:"40%",h:2}),(0,_.jsxs)("svg",{width:"12",height:"12",viewBox:"0 0 16 16",fill:"none",children:[(0,_.jsx)("rect",{x:"2",y:"3",width:"12",height:"11",rx:"1",stroke:"var(--agd-stroke)",strokeWidth:"1"}),(0,_.jsx)("line",{x1:"2",y1:"6",x2:"14",y2:"6",stroke:"var(--agd-stroke)",strokeWidth:"0.5"})]})]}),(0,_.jsxs)("div",{style:{flex:1,borderRadius:6,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",flexDirection:"column"},children:[(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",justifyContent:"space-between",padding:"4px 6px"},children:[(0,_.jsx)("span",{style:{fontSize:7,color:"var(--agd-stroke)"},children:"\u2039"}),(0,_.jsx)(H,{w:e*.25,h:2,strong:!0}),(0,_.jsx)("span",{style:{fontSize:7,color:"var(--agd-stroke)"},children:"\u203A"})]}),(0,_.jsx)("div",{style:{display:"grid",gridTemplateColumns:`repeat(${o}, 1fr)`,gap:1,padding:"0 4px",flex:1},children:Array.from({length:o*r},(s,l)=>(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:i},children:(0,_.jsx)("div",{style:{width:i*.5,height:i*.5,borderRadius:"50%",background:l===10?"var(--agd-bar)":"transparent"},children:(0,_.jsx)("div",{style:{width:"100%",height:"100%",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)("div",{style:{width:1.5,height:1.5,borderRadius:1,background:"var(--agd-bar-strong)",opacity:l===10?1:.25}})})})},l))})]})]})}function gw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",gap:t*.08,padding:4},children:[(0,_.jsx)("div",{style:{width:"100%",height:t*.2,borderRadius:4,background:"var(--agd-fill)"}}),(0,_.jsx)("div",{style:{width:"70%",height:Math.max(6,t*.1),borderRadius:3,background:"var(--agd-fill)"}}),(0,_.jsx)("div",{style:{width:"90%",height:Math.max(4,t*.06),borderRadius:3,background:"var(--agd-fill)"}}),(0,_.jsx)("div",{style:{width:"50%",height:Math.max(4,t*.06),borderRadius:3,background:"var(--agd-fill)"}})]})}function hw({width:e,height:t}){return(0,_.jsx)("div",{style:{height:"100%",display:"flex",alignItems:"center",gap:6},children:(0,_.jsxs)("div",{style:{height:"100%",flex:1,borderRadius:t/2,border:"1px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:`0 ${t*.3}px`,gap:4},children:[(0,_.jsx)(H,{w:"60%",h:2,strong:!0}),(0,_.jsx)("div",{style:{width:Math.max(6,t*.3),height:Math.max(6,t*.3),borderRadius:"50%",border:"1px solid var(--agd-stroke)",flexShrink:0,marginLeft:"auto"}})]})})}function yw({width:e,height:t}){let n=Math.min(e,t);return(0,_.jsx)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:(0,_.jsx)("path",{d:`M${e/2} ${(t-n)/2+n*.1}l${n*.12} ${n*.25} ${n*.28} ${n*.04}-${n*.2} ${n*.2} ${n*.05} ${n*.28}-${n*.25}-${n*.12}-${n*.25} ${n*.12} ${n*.05}-${n*.28}-${n*.2}-${n*.2} ${n*.28}-${n*.04}z`,stroke:"var(--agd-stroke)",strokeWidth:"1",fill:"var(--agd-fill)"})})}function xw({width:e,height:t}){let n=Math.min(e,t)/2-2;return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("circle",{cx:e/2,cy:t/2,r:n,stroke:"var(--agd-stroke)",strokeWidth:"1.5",opacity:".2"}),(0,_.jsx)("path",{d:`M${e/2} ${t/2-n}a${n} ${n} 0 0 1 ${n} ${n}`,stroke:"var(--agd-bar-strong)",strokeWidth:"1.5",strokeLinecap:"round"})]})}function vw({width:e,height:t}){let n=Math.min(36,t*.25,e*.12),o=Math.max(1,Math.min(3,Math.floor(t/80)));return(0,_.jsx)("div",{style:{display:"flex",flexDirection:"column",height:"100%",justifyContent:"space-around",padding:8},children:Array.from({length:o},(r,i)=>(0,_.jsxs)("div",{style:{display:"flex",gap:e*.04,alignItems:"flex-start"},children:[(0,_.jsx)(ft,{w:n,h:n,radius:n*.25}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsx)(H,{w:`${40+i*13%20}%`,h:3,strong:!0}),(0,_.jsx)(H,{w:`${60+i*17%25}%`,h:2})]})]},i))})}function ww({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/120))),o=Math.min(36,t*.25);return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",gap:t*.06,padding:t*.06},children:[(0,_.jsx)(H,{w:e*.3,h:4,strong:!0}),(0,_.jsx)("div",{style:{display:"flex",gap:e*.06,justifyContent:"center",flex:1,alignItems:"center"},children:Array.from({length:n},(r,i)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",gap:6},children:[(0,_.jsx)(xn,{size:o}),(0,_.jsx)(H,{w:e*.12,h:3,strong:!0}),(0,_.jsx)(H,{w:e*.08,h:2})]},i))})]})}function bw({width:e,height:t}){let n=Math.max(2,Math.min(3,Math.floor(t/80)));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",padding:e*.06,gap:t*.04},children:[(0,_.jsx)(H,{w:e*.5,h:Math.max(5,t*.04),strong:!0}),(0,_.jsx)(H,{w:e*.35,h:2}),(0,_.jsx)("div",{style:{width:"100%",display:"flex",flexDirection:"column",gap:t*.03,marginTop:t*.04},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(H,{w:Math.min(60,e*.2),h:2}),(0,_.jsx)(ft,{w:"100%",h:Math.min(32,t*.1),radius:4})]},r))}),(0,_.jsx)(ft,{w:"100%",h:Math.min(36,t*.12),radius:6,style:{marginTop:t*.03,background:"var(--agd-bar)"}}),(0,_.jsx)(H,{w:e*.4,h:2})]})}function kw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",padding:e*.04,gap:t*.03},children:[(0,_.jsx)(H,{w:e*.4,h:4,strong:!0}),(0,_.jsx)(H,{w:e*.6,h:2}),(0,_.jsxs)("div",{style:{display:"flex",gap:6,marginTop:t*.03},children:[(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(H,{w:50,h:2}),(0,_.jsx)(ft,{w:"100%",h:Math.min(28,t*.1),radius:4})]}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(H,{w:40,h:2}),(0,_.jsx)(ft,{w:"100%",h:Math.min(28,t*.1),radius:4})]})]}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(H,{w:50,h:2}),(0,_.jsx)(ft,{w:"100%",h:Math.min(28,t*.1),radius:4})]}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:3,flex:1},children:[(0,_.jsx)(H,{w:60,h:2}),(0,_.jsx)(ft,{w:"100%",h:"100%",radius:4})]}),(0,_.jsx)(ft,{w:Math.min(120,e*.3),h:Math.min(30,t*.1),radius:6,style:{alignSelf:"flex-end",background:"var(--agd-bar)"}})]})}var Sw={navigation:_v,hero:mv,sidebar:pv,footer:fv,modal:gv,card:hv,text:yv,image:xv,table:vv,list:wv,button:bv,input:kv,form:Sv,tabs:Cv,avatar:Ev,badge:Mv,header:Tv,section:Lv,grid:Rv,dropdown:Pv,toggle:Nv,search:Iv,toast:Dv,progress:$v,chart:Av,video:Fv,tooltip:Ov,breadcrumb:Bv,pagination:zv,divider:Hv,accordion:Wv,carousel:Uv,pricing:jv,testimonial:Yv,cta:Vv,alert:Xv,banner:Qv,stat:Gv,stepper:qv,tag:Kv,rating:Jv,map:Zv,timeline:ew,fileUpload:tw,codeBlock:nw,calendar:ow,notification:rw,productCard:iw,profile:sw,drawer:lw,popover:aw,logo:cw,faq:uw,gallery:dw,checkbox:_w,radio:mw,slider:pw,datePicker:fw,skeleton:gw,chip:hw,icon:yw,spinner:xw,feature:vw,team:ww,login:bw,contact:kw};function py({type:e,width:t,height:n,text:o}){let r=Sw[e];return r?(0,_.jsx)("div",{style:{width:"100%",height:"100%",padding:8,position:"relative",pointerEvents:"none"},children:(0,_.jsx)(r,{width:t,height:n,text:o})}):(0,_.jsx)("div",{style:{width:"100%",height:"100%",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)("span",{style:{fontSize:10,fontWeight:600,color:"var(--agd-text-3)",textTransform:"uppercase",letterSpacing:"0.06em",opacity:.5},children:e})})}var fy=`.styles-module__overlay___aWh-q svg[fill=none],
.styles-module__rearrangeOverlay___-3R3t svg[fill=none] {
  fill: none !important;
}
.styles-module__overlay___aWh-q svg[fill=none] :not([fill]),
.styles-module__rearrangeOverlay___-3R3t svg[fill=none] :not([fill]) {
  fill: none !important;
}

.styles-module__overlayExiting___iEmYr {
  opacity: 0 !important;
  transition: opacity 0.25s ease !important;
  pointer-events: none !important;
}

.styles-module__overlay___aWh-q {
  position: fixed;
  inset: 0;
  z-index: 99995;
  pointer-events: auto;
  cursor: default;
  animation: styles-module__overlayFadeIn___aECVy 0.15s ease;
  --agd-stroke: rgba(59, 130, 246, 0.35);
  --agd-fill: rgba(59, 130, 246, 0.06);
  --agd-bar: rgba(59, 130, 246, 0.18);
  --agd-bar-strong: rgba(59, 130, 246, 0.28);
  --agd-text-3: rgba(255, 255, 255, 0.6);
  --agd-surface: #fff;
}
.styles-module__overlay___aWh-q.styles-module__light___ORIft {
  --agd-surface: #fff;
}
.styles-module__overlay___aWh-q:not(.styles-module__light___ORIft) {
  --agd-surface: #141414;
}
.styles-module__overlay___aWh-q.styles-module__wireframe___itvQU {
  --agd-stroke: rgba(249, 115, 22, 0.35);
  --agd-fill: rgba(249, 115, 22, 0.06);
  --agd-bar: rgba(249, 115, 22, 0.18);
  --agd-bar-strong: rgba(249, 115, 22, 0.28);
}
.styles-module__overlay___aWh-q.styles-module__placing___45yD8 {
  cursor: crosshair;
}
.styles-module__overlay___aWh-q.styles-module__passthrough___xaFeE {
  pointer-events: none;
}

.styles-module__blankCanvas___t2Eue {
  position: fixed;
  inset: 0;
  z-index: 99994;
  background: #fff;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.25s ease;
}
.styles-module__blankCanvas___t2Eue.styles-module__visible___OKKqX {
  opacity: var(--canvas-opacity, 1);
  pointer-events: auto;
}
.styles-module__blankCanvas___t2Eue::after {
  content: "";
  position: absolute;
  inset: 0;
  background-image: radial-gradient(circle, rgba(0, 0, 0, 0.08) 1px, transparent 1px);
  background-size: 24px 24px;
  background-position: 12px 12px;
  pointer-events: none;
  transition: opacity 0.2s ease;
}
.styles-module__blankCanvas___t2Eue.styles-module__gridActive___OZ-cf::after {
  opacity: 1;
  background-image: radial-gradient(circle, rgba(0, 0, 0, 0.22) 1px, transparent 1px);
}

.styles-module__paletteHeader___-Q5gQ {
  padding: 0 1rem 0.375rem;
}

.styles-module__paletteHeaderTitle___oHqZC {
  font-size: 0.8125rem;
  font-weight: 500;
  color: #fff;
  letter-spacing: -0.0094em;
}
.styles-module__light___ORIft .styles-module__paletteHeaderTitle___oHqZC {
  color: rgba(0, 0, 0, 0.85);
}

.styles-module__paletteHeaderDesc___6i74T {
  font-size: 0.6875rem;
  font-weight: 300;
  color: rgba(255, 255, 255, 0.45);
  margin-top: 2px;
  line-height: 14px;
}
.styles-module__light___ORIft .styles-module__paletteHeaderDesc___6i74T {
  color: rgba(0, 0, 0, 0.45);
}
.styles-module__paletteHeaderDesc___6i74T a {
  color: rgba(255, 255, 255, 0.8);
  text-decoration: underline dotted;
  text-decoration-color: rgba(255, 255, 255, 0.2);
  text-underline-offset: 2px;
  transition: color 0.15s ease;
}
.styles-module__paletteHeaderDesc___6i74T a:hover {
  color: #fff;
}
.styles-module__light___ORIft .styles-module__paletteHeaderDesc___6i74T a {
  color: rgba(0, 0, 0, 0.6);
  text-decoration-color: rgba(0, 0, 0, 0.2);
}
.styles-module__light___ORIft .styles-module__paletteHeaderDesc___6i74T a:hover {
  color: rgba(0, 0, 0, 0.85);
}

.styles-module__wireframePurposeWrap___To-tS {
  display: grid;
  grid-template-rows: 1fr;
  transition: grid-template-rows 0.2s ease, opacity 0.15s ease;
  opacity: 1;
}
.styles-module__wireframePurposeWrap___To-tS.styles-module__collapsed___Ms9vS {
  grid-template-rows: 0fr;
  opacity: 0;
}

.styles-module__wireframePurposeInner___Lrahs {
  overflow: hidden;
}

.styles-module__wireframePurposeInput___7EtBN {
  display: block;
  width: calc(100% - 2rem);
  margin: 0.25rem 1rem 0.375rem;
  padding: 0.375rem 0.5rem;
  font-size: 0.8125rem;
  font-family: inherit;
  color: rgba(255, 255, 255, 0.85);
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 0.375rem;
  resize: none;
  outline: none;
  transition: border-color 0.15s ease;
  letter-spacing: -0.0094em;
}
.styles-module__wireframePurposeInput___7EtBN::placeholder {
  color: rgba(255, 255, 255, 0.3);
}
.styles-module__wireframePurposeInput___7EtBN:focus {
  border-color: rgba(255, 255, 255, 0.3);
  background: rgba(255, 255, 255, 0.05);
}
.styles-module__light___ORIft .styles-module__wireframePurposeInput___7EtBN {
  color: rgba(0, 0, 0, 0.7);
  background: rgba(0, 0, 0, 0.03);
  border-color: rgba(0, 0, 0, 0.1);
}
.styles-module__light___ORIft .styles-module__wireframePurposeInput___7EtBN::placeholder {
  color: rgba(0, 0, 0, 0.3);
}
.styles-module__light___ORIft .styles-module__wireframePurposeInput___7EtBN:focus {
  border-color: rgba(0, 0, 0, 0.25);
  background: rgba(0, 0, 0, 0.05);
}

.styles-module__canvasToggle___-QqSy {
  width: calc(100% - 2rem);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  margin: 0.25rem 1rem 0.25rem;
  padding: 0.375rem 0.5rem;
  border-radius: 0.5rem;
  cursor: pointer;
  border: 1px dashed rgba(255, 255, 255, 0.1);
  background: transparent;
  transition: background 0.15s ease, border-color 0.15s ease;
}
.styles-module__canvasToggle___-QqSy:hover {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.15);
}
.styles-module__canvasToggle___-QqSy.styles-module__active___hosp7 {
  background: #f97316;
  border-color: transparent;
  border-style: solid;
  box-shadow: none;
}
.styles-module__light___ORIft .styles-module__canvasToggle___-QqSy {
  border-color: rgba(0, 0, 0, 0.08);
}
.styles-module__light___ORIft .styles-module__canvasToggle___-QqSy:hover {
  background: rgba(0, 0, 0, 0.02);
  border-color: rgba(0, 0, 0, 0.12);
}
.styles-module__light___ORIft .styles-module__canvasToggle___-QqSy.styles-module__active___hosp7 {
  background: #f97316;
  border-color: transparent;
  border-style: solid;
  box-shadow: none;
}

.styles-module__canvasToggleIcon___7pJ82 {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: rgba(255, 255, 255, 0.35);
}
.styles-module__active___hosp7 .styles-module__canvasToggleIcon___7pJ82 {
  color: rgba(255, 255, 255, 0.85);
}
.styles-module__light___ORIft .styles-module__canvasToggleIcon___7pJ82 {
  color: rgba(0, 0, 0, 0.25);
}
.styles-module__light___ORIft .styles-module__active___hosp7 .styles-module__canvasToggleIcon___7pJ82 {
  color: rgba(255, 255, 255, 0.85);
}

.styles-module__canvasToggleLabel___OanpY {
  font-size: 0.8125rem;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.6);
  letter-spacing: -0.0094em;
}
.styles-module__active___hosp7 .styles-module__canvasToggleLabel___OanpY {
  color: #fff;
}
.styles-module__light___ORIft .styles-module__canvasToggleLabel___OanpY {
  color: rgba(0, 0, 0, 0.5);
}
.styles-module__light___ORIft .styles-module__active___hosp7 .styles-module__canvasToggleLabel___OanpY {
  color: #fff;
}

.styles-module__placement___zcxv8 {
  position: absolute;
  border: 1.5px dashed rgba(59, 130, 246, 0.4);
  border-radius: 6px;
  background: rgba(59, 130, 246, 0.08);
  cursor: grab;
  transition: box-shadow 0.15s, border-color 0.15s, opacity 0.15s ease, transform 0.15s ease;
  -webkit-user-select: none;
  user-select: none;
  pointer-events: auto;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.08);
  animation: styles-module__placementEnter___TdRhf 0.25s cubic-bezier(0.34, 1.2, 0.64, 1);
}
.styles-module__placement___zcxv8:active {
  cursor: grabbing;
}
.styles-module__placement___zcxv8:hover {
  border-color: rgba(59, 130, 246, 0.5);
  background: rgba(59, 130, 246, 0.1);
  box-shadow: 0 2px 8px rgba(59, 130, 246, 0.12);
}
.styles-module__placement___zcxv8.styles-module__selected___6yrp6 {
  border-color: #3c82f7;
  border-style: solid;
  background: rgba(59, 130, 246, 0.1);
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.15), 0 2px 8px rgba(59, 130, 246, 0.15);
}
.styles-module__placement___zcxv8.styles-module__selected___6yrp6:hover {
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.15), 0 2px 8px rgba(59, 130, 246, 0.15);
}
.styles-module__wireframe___itvQU .styles-module__placement___zcxv8 {
  border-color: rgba(249, 115, 22, 0.4);
  background: rgba(249, 115, 22, 0.08);
}
.styles-module__wireframe___itvQU .styles-module__placement___zcxv8:hover {
  border-color: rgba(249, 115, 22, 0.5);
  background: rgba(249, 115, 22, 0.1);
  box-shadow: 0 2px 8px rgba(249, 115, 22, 0.12);
}
.styles-module__wireframe___itvQU .styles-module__placement___zcxv8.styles-module__selected___6yrp6 {
  border-color: #f97316;
  background: rgba(249, 115, 22, 0.1);
  box-shadow: 0 0 0 2px rgba(249, 115, 22, 0.15), 0 2px 8px rgba(249, 115, 22, 0.15);
}
.styles-module__wireframe___itvQU .styles-module__placement___zcxv8.styles-module__selected___6yrp6:hover {
  box-shadow: 0 0 0 2px rgba(249, 115, 22, 0.15), 0 2px 8px rgba(249, 115, 22, 0.15);
}
.styles-module__placement___zcxv8.styles-module__dragging___le6KZ {
  opacity: 0.85;
  z-index: 50;
}
.styles-module__placement___zcxv8.styles-module__exiting___YrM8F {
  opacity: 0;
  transform: scale(0.97);
  pointer-events: none;
  animation: none;
  transition: opacity 0.2s ease, transform 0.2s cubic-bezier(0.32, 0.72, 0, 1);
}

.styles-module__placementContent___f64A4 {
  width: 100%;
  height: 100%;
  overflow: hidden;
  pointer-events: none;
}

.styles-module__placementLabel___0KvWl {
  position: absolute;
  top: -18px;
  left: 0;
  font-size: 10px;
  font-weight: 600;
  color: rgba(59, 130, 246, 0.7);
  white-space: nowrap;
  pointer-events: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  text-shadow: 0 0 4px rgba(255, 255, 255, 0.8), 0 0 8px rgba(255, 255, 255, 0.5);
}
.styles-module__selected___6yrp6 .styles-module__placementLabel___0KvWl {
  color: #3c82f7;
}
.styles-module__wireframe___itvQU .styles-module__placementLabel___0KvWl {
  color: rgba(249, 115, 22, 0.7);
}
.styles-module__wireframe___itvQU .styles-module__selected___6yrp6 .styles-module__placementLabel___0KvWl {
  color: #f97316;
}

.styles-module__placementAnnotation___78pTr {
  position: absolute;
  bottom: -18px;
  left: 0;
  right: 0;
  font-weight: 450;
  color: rgba(0, 0, 0, 0.5);
  font-size: 10px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  text-shadow: 0 0 4px rgba(255, 255, 255, 0.9), 0 0 8px rgba(255, 255, 255, 0.6);
  opacity: 0;
  transform: translateY(-2px);
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.styles-module__placementAnnotation___78pTr.styles-module__annotationVisible___mrUyA {
  opacity: 1;
  transform: translateY(0);
}

.styles-module__sectionAnnotation___aUIs0 {
  position: absolute;
  bottom: -18px;
  left: 0;
  right: 0;
  font-weight: 450;
  color: rgba(59, 130, 246, 0.6);
  font-size: 10px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  text-shadow: 0 0 4px rgba(255, 255, 255, 0.9), 0 0 8px rgba(255, 255, 255, 0.6);
  opacity: 0;
  transform: translateY(-2px);
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.styles-module__sectionAnnotation___aUIs0.styles-module__annotationVisible___mrUyA {
  opacity: 1;
  transform: translateY(0);
}

.styles-module__handle___Ikbxm {
  position: absolute;
  width: 8px;
  height: 8px;
  background: #fff;
  border: 1.5px solid #3c82f7;
  border-radius: 2px;
  z-index: 12;
  box-shadow: 0 0 0 0.5px rgba(0, 0, 0, 0.1), 0 1px 2px rgba(0, 0, 0, 0.12);
  opacity: 0;
  transform: scale(0.3);
  pointer-events: none;
  will-change: opacity, transform;
  transition: opacity 0.2s ease-out, transform 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
}
.styles-module__placement___zcxv8:hover .styles-module__handle___Ikbxm, .styles-module__sectionOutline___s0hy-:hover .styles-module__handle___Ikbxm, .styles-module__ghostOutline___po-kO:hover .styles-module__handle___Ikbxm, .styles-module__placement___zcxv8:active .styles-module__handle___Ikbxm, .styles-module__sectionOutline___s0hy-:active .styles-module__handle___Ikbxm, .styles-module__ghostOutline___po-kO:active .styles-module__handle___Ikbxm, .styles-module__selected___6yrp6 .styles-module__handle___Ikbxm {
  opacity: 1;
  transform: scale(1);
  pointer-events: auto;
}
.styles-module__sectionOutline___s0hy- .styles-module__handle___Ikbxm {
  border-color: inherit;
}
.styles-module__wireframe___itvQU .styles-module__handle___Ikbxm {
  border-color: #f97316;
}

.styles-module__handleNw___4TMIj {
  top: -4px;
  left: -4px;
  cursor: nw-resize;
}

.styles-module__handleNe___mnsTh {
  top: -4px;
  right: -4px;
  cursor: ne-resize;
}

.styles-module__handleSe___oSFnk {
  bottom: -4px;
  right: -4px;
  cursor: se-resize;
}

.styles-module__handleSw___pi--Z {
  bottom: -4px;
  left: -4px;
  cursor: sw-resize;
}

.styles-module__handleN___aBA-Q,
.styles-module__handleE___0hM5u,
.styles-module__handleS___JjDRv,
.styles-module__handleW___ERWGQ {
  opacity: 0 !important;
  pointer-events: none !important;
}

.styles-module__edgeHandle___XxXdT {
  position: absolute;
  z-index: 11;
  display: flex;
  align-items: center;
  justify-content: center;
}
.styles-module__edgeHandle___XxXdT::after {
  content: "";
  position: absolute;
  border-radius: 4px;
  background: #3c82f7;
}
.styles-module__wireframe___itvQU .styles-module__edgeHandle___XxXdT::after {
  background: #f97316;
}
.styles-module__edgeHandle___XxXdT::after {
  opacity: 0;
  transition: opacity 0.1s ease, transform 0.1s ease;
  transform: scale(0.8);
}
.styles-module__edgeHandle___XxXdT:hover::after {
  opacity: 0.85;
  transform: scale(1);
}
.styles-module__edgeHandle___XxXdT svg {
  position: relative;
  z-index: 1;
  opacity: 0;
  transition: opacity 0.1s ease;
  filter: drop-shadow(0 0 2px var(--agd-surface));
}
.styles-module__edgeHandle___XxXdT:hover svg {
  opacity: 1;
}

.styles-module__edgeN___-JJDj,
.styles-module__edgeS___66lMX {
  left: 12px;
  right: 12px;
  height: 12px;
  cursor: n-resize;
}
.styles-module__edgeN___-JJDj::after,
.styles-module__edgeS___66lMX::after {
  width: 24px;
  height: 4px;
}

.styles-module__edgeN___-JJDj {
  top: -6px;
}

.styles-module__edgeS___66lMX {
  bottom: -6px;
  cursor: s-resize;
}

.styles-module__edgeE___1bGDa,
.styles-module__edgeW___lHQNo {
  top: 12px;
  bottom: 12px;
  width: 12px;
  cursor: e-resize;
}
.styles-module__edgeE___1bGDa::after,
.styles-module__edgeW___lHQNo::after {
  width: 4px;
  height: 24px;
}

.styles-module__edgeE___1bGDa {
  right: -6px;
}

.styles-module__edgeW___lHQNo {
  left: -6px;
  cursor: w-resize;
}

.styles-module__deleteButton___LkGCb {
  position: absolute;
  top: -8px;
  right: -8px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.9);
  backdrop-filter: blur(8px);
  border: 1px solid rgba(0, 0, 0, 0.08);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  color: rgba(0, 0, 0, 0.35);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  line-height: 1;
  z-index: 15;
  pointer-events: none;
  opacity: 0;
  transform: scale(0.8);
  will-change: opacity, transform;
  transition: opacity 0.2s ease-out, transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1), background 0.12s ease, color 0.12s ease, border-color 0.12s ease, box-shadow 0.12s ease;
}
.styles-module__placement___zcxv8:hover .styles-module__deleteButton___LkGCb, .styles-module__selected___6yrp6 .styles-module__deleteButton___LkGCb, .styles-module__sectionOutline___s0hy-:hover .styles-module__deleteButton___LkGCb, .styles-module__sectionOutline___s0hy-.styles-module__selected___6yrp6 .styles-module__deleteButton___LkGCb, .styles-module__ghostOutline___po-kO:hover .styles-module__deleteButton___LkGCb, .styles-module__ghostOutline___po-kO.styles-module__selected___6yrp6 .styles-module__deleteButton___LkGCb {
  opacity: 1;
  transform: scale(1);
  pointer-events: auto;
}
.styles-module__deleteButton___LkGCb:hover {
  background: #ef4444;
  color: #fff;
  border-color: #ef4444;
  box-shadow: 0 1px 4px rgba(239, 68, 68, 0.3);
  transform: scale(1.1);
}
.styles-module__overlay___aWh-q:not(.styles-module__light___ORIft) .styles-module__deleteButton___LkGCb, .styles-module__rearrangeOverlay___-3R3t:not(.styles-module__light___ORIft) .styles-module__deleteButton___LkGCb {
  background: rgba(40, 40, 40, 0.9);
  border-color: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.5);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
}
.styles-module__overlay___aWh-q:not(.styles-module__light___ORIft) .styles-module__deleteButton___LkGCb:hover, .styles-module__rearrangeOverlay___-3R3t:not(.styles-module__light___ORIft) .styles-module__deleteButton___LkGCb:hover {
  background: #ef4444;
  color: #fff;
  border-color: #ef4444;
}

.styles-module__drawBox___BrVAa {
  position: fixed;
  pointer-events: none;
  z-index: 99996;
  border: 2px solid #3c82f7;
  border-radius: 6px;
  background: rgba(59, 130, 246, 0.15);
}

.styles-module__selectBox___Iu8kB {
  position: fixed;
  pointer-events: none;
  z-index: 99996;
  border: 1px dashed #3c82f7;
  background: rgba(59, 130, 246, 0.08);
  border-radius: 2px;
}

.styles-module__sizeIndicator___7zJ4y {
  position: fixed;
  pointer-events: none;
  z-index: 100001;
  font-size: 10px;
  color: #fff;
  background: #3c82f7;
  padding: 2px 6px;
  border-radius: 4px;
  white-space: nowrap;
  font-weight: 500;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2);
}

.styles-module__guideLine___DUQY2 {
  pointer-events: none;
  z-index: 100001;
  background: #f0f;
  opacity: 0.5;
}

.styles-module__dragPreview___onPbU {
  position: fixed;
  z-index: 100002;
  pointer-events: none;
  border: 1.5px dashed #3c82f7;
  border-radius: 6px;
  background: rgba(59, 130, 246, 0.1);
  backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 9px;
  font-weight: 600;
  color: #3c82f7;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  box-shadow: 0 4px 16px rgba(59, 130, 246, 0.15);
  transition: width 0.08s ease, height 0.08s ease, opacity 0.08s ease;
}

.styles-module__dragPreviewWireframe___jsg0G {
  border-color: #f97316;
  background: rgba(249, 115, 22, 0.1);
  color: #f97316;
  box-shadow: 0 4px 16px rgba(249, 115, 22, 0.15);
}

.styles-module__palette___C7iSH {
  position: absolute;
  right: 5px;
  bottom: calc(100% + 0.5rem);
  width: 256px;
  overflow: hidden;
  background: #1c1c1c;
  border: none;
  border-radius: 1rem;
  padding: 13px 0 16px;
  box-shadow: 0 1px 8px rgba(0, 0, 0, 0.25), 0 0 0 1px rgba(0, 0, 0, 0.04);
  z-index: 100001;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  cursor: default;
  opacity: 0;
  filter: blur(5px);
}
.styles-module__palette___C7iSH .styles-module__paletteItem___6TlnA,
.styles-module__palette___C7iSH .styles-module__paletteItemLabel___6ncO4,
.styles-module__palette___C7iSH .styles-module__paletteSectionTitle___PqnjX,
.styles-module__palette___C7iSH .styles-module__paletteFooter___QYnAG {
  transition: background 0.25s ease, color 0.25s ease, border-color 0.25s ease;
}
.styles-module__palette___C7iSH {
  opacity: 0;
  transform: translateY(var(--panel-offset-y, 4px)) scale(0.98);
  transform-origin: var(--panel-origin, bottom right);
  filter: blur(2px);
  pointer-events: none;
  visibility: hidden;
  transition: opacity 120ms cubic-bezier(0.25, 0.46, 0.45, 0.94), transform 120ms cubic-bezier(0.25, 0.46, 0.45, 0.94), filter 120ms cubic-bezier(0.25, 0.46, 0.45, 0.94);
}
.styles-module__palette___C7iSH[data-panel-present=true] {
  visibility: visible;
}
.styles-module__palette___C7iSH[data-panel-open=true] {
  opacity: 1;
  transform: translateY(0) scale(1);
  filter: blur(0);
  pointer-events: auto;
  transition-duration: 160ms;
}
@media (prefers-reduced-motion: reduce) {
  .styles-module__palette___C7iSH {
    transition: none;
    transform: none;
    filter: none;
  }
}
.styles-module__palette___C7iSH.styles-module__light___ORIft {
  background: #fff;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08), 0 4px 16px rgba(0, 0, 0, 0.06), 0 0 0 1px rgba(0, 0, 0, 0.04);
}

.styles-module__paletteSection___V8DEA {
  padding: 0 1rem;
}
.styles-module__paletteSection___V8DEA + .styles-module__paletteSection___V8DEA {
  margin-top: 0.5rem;
  padding-top: 0.5rem;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
}
.styles-module__light___ORIft .styles-module__paletteSection___V8DEA + .styles-module__paletteSection___V8DEA {
  border-top-color: rgba(0, 0, 0, 0.07);
}

.styles-module__paletteSectionTitle___PqnjX {
  font-size: 0.6875rem;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.5);
  letter-spacing: -0.0094em;
  padding: 0 0 3px 3px;
}
.styles-module__light___ORIft .styles-module__paletteSectionTitle___PqnjX {
  color: rgba(0, 0, 0, 0.4);
}

.styles-module__paletteItem___6TlnA {
  width: 100%;
  text-align: left;
  background: transparent;
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.25rem 0.25rem;
  margin-bottom: 1px;
  border-radius: 0.375rem;
  cursor: pointer;
  transition: background-color 0.15s ease, border-color 0.15s ease;
  border: 1px solid transparent;
  -webkit-user-select: none;
  user-select: none;
  min-height: 24px;
}
.styles-module__paletteItem___6TlnA:hover {
  background: rgba(255, 255, 255, 0.1);
}
.styles-module__paletteItem___6TlnA.styles-module__active___hosp7 {
  background: #3c82f7;
  border-color: transparent;
}
.styles-module__paletteItem___6TlnA.styles-module__wireframe___itvQU.styles-module__active___hosp7 {
  background: #f97316;
}
.styles-module__light___ORIft .styles-module__paletteItem___6TlnA:hover {
  background: rgba(0, 0, 0, 0.05);
}
.styles-module__light___ORIft .styles-module__paletteItem___6TlnA.styles-module__active___hosp7 {
  background: #3c82f7;
  border-color: transparent;
}
.styles-module__light___ORIft .styles-module__paletteItem___6TlnA.styles-module__wireframe___itvQU.styles-module__active___hosp7 {
  background: #f97316;
}

.styles-module__paletteItemIcon___0NPQK {
  width: 20px;
  height: 16px;
  border-radius: 2px;
  border: 1px dashed rgba(255, 255, 255, 0.15);
  background: rgba(255, 255, 255, 0.04);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  overflow: hidden;
  color: rgba(255, 255, 255, 0.45);
}
.styles-module__paletteItemIcon___0NPQK svg {
  display: block;
  width: 20px;
  height: 16px;
}
.styles-module__active___hosp7 .styles-module__paletteItemIcon___0NPQK {
  border-color: rgba(255, 255, 255, 0.3);
  background: rgba(255, 255, 255, 0.15);
  color: #fff;
}
.styles-module__light___ORIft .styles-module__paletteItemIcon___0NPQK {
  border-color: rgba(0, 0, 0, 0.12);
  background: rgba(0, 0, 0, 0.02);
  color: rgba(0, 0, 0, 0.4);
}
.styles-module__light___ORIft .styles-module__active___hosp7 .styles-module__paletteItemIcon___0NPQK {
  border-color: rgba(255, 255, 255, 0.3);
  background: rgba(255, 255, 255, 0.15);
  color: #fff;
}

.styles-module__paletteItemLabel___6ncO4 {
  font-size: 0.8125rem;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.85);
  letter-spacing: -0.0094em;
  line-height: 1;
  min-width: 0;
}
.styles-module__active___hosp7 .styles-module__paletteItemLabel___6ncO4 {
  color: #fff;
  font-weight: 600;
}
.styles-module__light___ORIft .styles-module__paletteItemLabel___6ncO4 {
  color: rgba(0, 0, 0, 0.7);
}
.styles-module__light___ORIft .styles-module__active___hosp7 .styles-module__paletteItemLabel___6ncO4 {
  color: #fff;
  font-weight: 600;
}

.styles-module__placeScroll___7sClM {
  max-height: 240px;
  overflow-y: auto;
  overflow-x: hidden;
  padding-top: 0.25rem;
}
.styles-module__placeScroll___7sClM.styles-module__fadeTop___KT9tF {
  -webkit-mask-image: linear-gradient(to bottom, transparent 0, black 32px);
  mask-image: linear-gradient(to bottom, transparent 0, black 32px);
}
.styles-module__placeScroll___7sClM.styles-module__fadeBottom___x3ShT {
  -webkit-mask-image: linear-gradient(to bottom, black calc(100% - 32px), transparent 100%);
  mask-image: linear-gradient(to bottom, black calc(100% - 32px), transparent 100%);
}
.styles-module__placeScroll___7sClM.styles-module__fadeTop___KT9tF.styles-module__fadeBottom___x3ShT {
  -webkit-mask-image: linear-gradient(to bottom, transparent 0, black 32px, black calc(100% - 32px), transparent 100%);
  mask-image: linear-gradient(to bottom, transparent 0, black 32px, black calc(100% - 32px), transparent 100%);
}
.styles-module__placeScroll___7sClM::-webkit-scrollbar {
  width: 3px;
}
.styles-module__placeScroll___7sClM::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.12);
  border-radius: 2px;
}
.styles-module__light___ORIft .styles-module__placeScroll___7sClM::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.1);
}

.styles-module__paletteFooterWrap___71-fI {
  display: grid;
  grid-template-rows: 1fr;
  transition: grid-template-rows 0.25s cubic-bezier(0.32, 0.72, 0, 1);
}
.styles-module__paletteFooterWrap___71-fI.styles-module__footerHidden___fJUik {
  grid-template-rows: 0fr;
}

.styles-module__paletteFooterInnerContent___VC26h {
  opacity: 1;
  transform: translateY(0);
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.styles-module__footerHidden___fJUik .styles-module__paletteFooterInnerContent___VC26h {
  opacity: 0;
  transform: translateY(4px);
}

.styles-module__paletteFooterInner___dfylY {
  overflow: hidden;
}

.styles-module__paletteFooter___QYnAG {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 24px;
  padding: 0 1rem;
  margin-top: 0.5rem;
  padding-top: 0.5rem;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
}
.styles-module__light___ORIft .styles-module__paletteFooter___QYnAG {
  border-top-color: rgba(0, 0, 0, 0.07);
}

.styles-module__paletteFooterCount___D3Fia {
  font-size: 0.8125rem;
  font-weight: 400;
  letter-spacing: -0.0094em;
  color: rgba(255, 255, 255, 0.5);
}
.styles-module__light___ORIft .styles-module__paletteFooterCount___D3Fia {
  color: rgba(0, 0, 0, 0.5);
}

.styles-module__paletteFooterClear___ybBoa {
  font-size: 0.8125rem;
  font-weight: 400;
  letter-spacing: -0.0094em;
  color: rgba(255, 255, 255, 0.5);
  background: none;
  border: none;
  cursor: pointer;
  padding: 0;
  font-family: inherit;
  transition: color 0.15s ease;
}
.styles-module__paletteFooterClear___ybBoa:hover {
  color: rgba(255, 255, 255, 0.7);
}
.styles-module__light___ORIft .styles-module__paletteFooterClear___ybBoa {
  color: rgba(0, 0, 0, 0.5);
}
.styles-module__light___ORIft .styles-module__paletteFooterClear___ybBoa:hover {
  color: rgba(0, 0, 0, 0.6);
}

.styles-module__paletteFooterActions___fLzv8 {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.styles-module__rollingWrap___S75jM {
  display: inline-block;
  overflow: hidden;
  height: 1.15em;
  position: relative;
  vertical-align: bottom;
}

.styles-module__rollingNum___1RKDx {
  position: absolute;
  left: 0;
  top: 0;
}

.styles-module__exitUp___AFDRW {
  animation: styles-module__numExitUp___FRQqx 0.25s cubic-bezier(0.32, 0.72, 0, 1) forwards;
}

.styles-module__enterUp___CPlXb {
  animation: styles-module__numEnterUp___2Yd-w 0.25s cubic-bezier(0.32, 0.72, 0, 1) forwards;
}

.styles-module__exitDown___-1yAy {
  animation: styles-module__numExitDown___xm5by 0.25s cubic-bezier(0.32, 0.72, 0, 1) forwards;
}

.styles-module__enterDown___DDuFR {
  animation: styles-module__numEnterDown___hpxBk 0.25s cubic-bezier(0.32, 0.72, 0, 1) forwards;
}

@keyframes styles-module__numExitUp___FRQqx {
  from {
    transform: translateY(0);
    opacity: 1;
  }
  to {
    transform: translateY(-110%);
    opacity: 0;
  }
}
@keyframes styles-module__numEnterUp___2Yd-w {
  from {
    transform: translateY(110%);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}
@keyframes styles-module__numExitDown___xm5by {
  from {
    transform: translateY(0);
    opacity: 1;
  }
  to {
    transform: translateY(110%);
    opacity: 0;
  }
}
@keyframes styles-module__numEnterDown___hpxBk {
  from {
    transform: translateY(-110%);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}
.styles-module__rearrangeOverlay___-3R3t {
  position: fixed;
  inset: 0;
  z-index: 99995;
  pointer-events: none;
  cursor: default;
  -webkit-user-select: none;
  user-select: none;
  animation: styles-module__overlayFadeIn___aECVy 0.15s ease;
}

.styles-module__hoverHighlight___8eT-v {
  position: fixed;
  pointer-events: none;
  z-index: 99994;
  border: 2px dashed rgba(59, 130, 246, 0.5);
  border-radius: 4px;
  background: rgba(59, 130, 246, 0.06);
  animation: styles-module__highlightFadeIn___Lg7KY 0.12s ease;
}

.styles-module__sectionOutline___s0hy- {
  position: fixed;
  border: 2px solid;
  border-radius: 4px;
  cursor: grab;
}
.styles-module__sectionOutline___s0hy-:active {
  cursor: grabbing;
}
.styles-module__sectionOutline___s0hy- {
  transition: box-shadow 0.15s, border-color 0.3s, background-color 0.3s, border-style 0s;
  -webkit-user-select: none;
  user-select: none;
  pointer-events: auto;
  animation: styles-module__sectionEnter___-8BXT 0.2s ease;
}
.styles-module__sectionOutline___s0hy-:hover {
  box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.1), 0 4px 12px rgba(0, 0, 0, 0.15);
}
.styles-module__sectionOutline___s0hy-.styles-module__selected___6yrp6 {
  border-style: solid;
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.15), 0 2px 8px rgba(59, 130, 246, 0.15);
}
.styles-module__sectionOutline___s0hy-.styles-module__selected___6yrp6:hover {
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.15), 0 2px 8px rgba(59, 130, 246, 0.15);
}
.styles-module__sectionOutline___s0hy-.styles-module__settled___b5U5o:not(.styles-module__selected___6yrp6) {
  border: 1.5px dashed rgba(150, 150, 150, 0.35);
  background-color: transparent !important;
  box-shadow: none;
}
.styles-module__sectionOutline___s0hy-.styles-module__settled___b5U5o:not(.styles-module__selected___6yrp6):hover {
  border-color: rgba(150, 150, 150, 0.6);
  box-shadow: none;
}
.styles-module__sectionOutline___s0hy-.styles-module__settled___b5U5o:not(.styles-module__selected___6yrp6) .styles-module__sectionLabel___F80HQ {
  opacity: 0;
  transition: opacity 0.15s ease;
}
.styles-module__sectionOutline___s0hy-.styles-module__settled___b5U5o:not(.styles-module__selected___6yrp6):hover .styles-module__sectionLabel___F80HQ {
  opacity: 1;
}
.styles-module__sectionOutline___s0hy-.styles-module__settled___b5U5o:not(.styles-module__selected___6yrp6) .styles-module__movedBadge___s8z-q,
.styles-module__sectionOutline___s0hy-.styles-module__settled___b5U5o:not(.styles-module__selected___6yrp6) .styles-module__sectionDimensions___RcJSL {
  opacity: 0;
  transition: opacity 0.15s ease;
}
.styles-module__sectionOutline___s0hy-.styles-module__settled___b5U5o:not(.styles-module__selected___6yrp6):hover .styles-module__sectionDimensions___RcJSL {
  opacity: 1;
}
.styles-module__sectionOutline___s0hy-.styles-module__exiting___YrM8F {
  opacity: 0;
  transform: scale(0.97);
  pointer-events: none;
  animation: none;
  transition: opacity 0.2s ease, transform 0.2s cubic-bezier(0.32, 0.72, 0, 1);
}

.styles-module__sectionLabel___F80HQ {
  position: absolute;
  top: 4px;
  left: 4px;
  font-size: 10px;
  font-weight: 600;
  color: #fff;
  padding: 2px 8px;
  border-radius: 4px;
  white-space: nowrap;
  pointer-events: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
  max-width: calc(100% - 8px);
  overflow: hidden;
  text-overflow: ellipsis;
}

.styles-module__movedBadge___s8z-q {
  position: absolute;
  bottom: 22px;
  right: 4px;
  font-size: 9px;
  font-weight: 700;
  color: #fff;
  background: #22c55e;
  padding: 2px 6px;
  border-radius: 4px;
  white-space: nowrap;
  pointer-events: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
  opacity: 0;
  transform: scale(0.8);
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.styles-module__movedBadge___s8z-q.styles-module__badgeVisible___npbdS {
  opacity: 1;
  transform: scale(1);
  transition: opacity 0.2s cubic-bezier(0.34, 1.2, 0.64, 1), transform 0.2s cubic-bezier(0.34, 1.2, 0.64, 1);
}

.styles-module__resizedBadge___u51V8 {
  background: #3c82f7;
  bottom: 40px;
}

.styles-module__sectionDimensions___RcJSL {
  position: absolute;
  bottom: 4px;
  right: 4px;
  font-size: 9px;
  font-weight: 500;
  color: rgba(255, 255, 255, 0.7);
  background: rgba(0, 0, 0, 0.5);
  padding: 1px 5px;
  border-radius: 3px;
  white-space: nowrap;
  pointer-events: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
}
.styles-module__light___ORIft .styles-module__sectionDimensions___RcJSL {
  color: rgba(0, 0, 0, 0.5);
  background: rgba(255, 255, 255, 0.7);
}

.styles-module__wireframeNotice___4GJyB {
  position: fixed;
  bottom: 16px;
  left: 24px;
  z-index: 99995;
  font-size: 9.5px;
  font-weight: 400;
  color: rgba(0, 0, 0, 0.4);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  pointer-events: auto;
  animation: styles-module__overlayFadeIn___aECVy 0.3s ease;
  line-height: 1.5;
  max-width: 280px;
}

.styles-module__wireframeOpacityRow___CJXzi {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.styles-module__wireframeOpacityLabel___afkfT {
  font-size: 9px;
  font-weight: 500;
  color: rgba(0, 0, 0, 0.32);
  letter-spacing: 0.02em;
  white-space: nowrap;
  -webkit-user-select: none;
  user-select: none;
}

.styles-module__wireframeOpacitySlider___YcoEs {
  -webkit-appearance: none;
  appearance: none;
  width: 56px;
  height: 4px;
  background: rgba(0, 0, 0, 0.08);
  border-radius: 2px;
  outline: none;
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.15s ease;
}
.styles-module__wireframeOpacitySlider___YcoEs:hover {
  background: rgba(0, 0, 0, 0.13);
}
.styles-module__wireframeOpacitySlider___YcoEs::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #f97316;
  cursor: pointer;
  transition: background 0.15s ease;
}
.styles-module__wireframeOpacitySlider___YcoEs::-webkit-slider-thumb:hover {
  background: rgb(88.0082041185%, 37.39404381%, 2.2663056855%);
}
.styles-module__wireframeOpacitySlider___YcoEs::-moz-range-thumb {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #f97316;
  border: none;
  cursor: pointer;
}
.styles-module__wireframeOpacitySlider___YcoEs::-moz-range-track {
  background: rgba(0, 0, 0, 0.08);
  height: 4px;
  border-radius: 2px;
}

.styles-module__wireframeNoticeTitleRow___PJqyG {
  display: flex;
  align-items: center;
  gap: 0;
  margin-bottom: 2px;
}

.styles-module__wireframeNoticeTitle___okr08 {
  font-weight: 600;
  color: rgba(0, 0, 0, 0.55);
}

.styles-module__wireframeNoticeDivider___PNKQ6 {
  width: 1px;
  height: 8px;
  background: rgba(0, 0, 0, 0.12);
  margin: 0 8px;
  flex-shrink: 0;
}

.styles-module__wireframeStartOver___YFk-I {
  font-size: 9.5px;
  font-weight: 500;
  color: rgba(0, 0, 0, 0.35);
  cursor: pointer;
  background: none;
  border: none;
  padding: 0;
  font-family: inherit;
  text-decoration: none;
  transition: color 0.12s ease;
  white-space: nowrap;
}
.styles-module__wireframeStartOver___YFk-I:hover {
  color: rgba(0, 0, 0, 0.6);
}

.styles-module__ghostOutline___po-kO {
  position: fixed;
  border: 1.5px dashed rgba(59, 130, 246, 0.4);
  border-radius: 4px;
  background: rgba(59, 130, 246, 0.04);
  cursor: grab;
  opacity: 0.5;
  -webkit-user-select: none;
  user-select: none;
  pointer-events: auto;
  animation: styles-module__ghostEnter___EC3Mb 0.25s ease;
  transition: box-shadow 0.15s, border-color 0.3s, opacity 0.25s;
}
.styles-module__ghostOutline___po-kO:active {
  cursor: grabbing;
}
.styles-module__ghostOutline___po-kO:hover {
  opacity: 0.7;
  box-shadow: 0 0 0 1px rgba(59, 130, 246, 0.1), 0 4px 12px rgba(0, 0, 0, 0.08);
}
.styles-module__ghostOutline___po-kO.styles-module__selected___6yrp6 {
  opacity: 1;
  border-style: solid;
  border-width: 2px;
  border-color: #3c82f7;
  background: rgba(59, 130, 246, 0.08);
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.15), 0 2px 8px rgba(59, 130, 246, 0.15);
}
.styles-module__ghostOutline___po-kO.styles-module__exiting___YrM8F {
  opacity: 0;
  transform: scale(0.97);
  pointer-events: none;
  animation: none;
  transition: opacity 0.2s ease, transform 0.2s cubic-bezier(0.32, 0.72, 0, 1);
}

.styles-module__ghostBadge___tsQUK {
  position: absolute;
  bottom: calc(100% + 4px);
  left: -1px;
  font-size: 9px;
  font-weight: 600;
  color: rgba(59, 130, 246, 0.9);
  background: rgba(59, 130, 246, 0.08);
  border: 1px solid rgba(59, 130, 246, 0.2);
  padding: 1px 5px;
  border-radius: 3px;
  white-space: nowrap;
  pointer-events: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  letter-spacing: 0.02em;
  line-height: 1.2;
  animation: styles-module__badgeSlideIn___typJ7 0.2s ease both;
}

@keyframes styles-module__badgeSlideIn___typJ7 {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
.styles-module__ghostBadgeExtra___6CVoD {
  display: inline;
  animation: styles-module__badgeExtraIn___i4W8F 0.2s ease both;
}

@keyframes styles-module__badgeExtraIn___i4W8F {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
.styles-module__originalOutline___Y6DD1 {
  position: fixed;
  border: 1.5px dashed rgba(150, 150, 150, 0.3);
  border-radius: 4px;
  background: transparent;
  pointer-events: none;
  -webkit-user-select: none;
  user-select: none;
  animation: styles-module__sectionEnter___-8BXT 0.2s ease;
}

.styles-module__originalLabel___HqI9g {
  position: absolute;
  top: 4px;
  left: 4px;
  font-size: 9px;
  font-weight: 500;
  color: rgba(150, 150, 150, 0.5);
  padding: 1px 6px;
  border-radius: 3px;
  white-space: nowrap;
  pointer-events: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  background: rgba(150, 150, 150, 0.08);
}

.styles-module__connectorSvg___Lovld {
  position: fixed;
  inset: 0;
  width: 100vw;
  height: 100vh;
  pointer-events: none;
  z-index: 99996;
}

.styles-module__connectorLine___XeWh- {
  transition: opacity 0.2s ease;
  animation: styles-module__connectorDraw___8sK5I 0.3s ease both;
}

.styles-module__connectorDot___yvf7C {
  transform-box: fill-box;
  transform-origin: center;
  animation: styles-module__connectorDotIn___NwTUq 0.25s cubic-bezier(0.34, 1.56, 0.64, 1) 0.15s both;
}

@keyframes styles-module__connectorDraw___8sK5I {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
@keyframes styles-module__connectorDotIn___NwTUq {
  from {
    transform: scale(0);
    opacity: 0;
  }
  to {
    transform: scale(1);
    opacity: 1;
  }
}
.styles-module__connectorExiting___2lLOs {
  animation: styles-module__connectorOut___5QoPl 0.2s ease forwards;
}
.styles-module__connectorExiting___2lLOs .styles-module__connectorDot___yvf7C {
  animation: styles-module__connectorDotOut___FEq7e 0.2s ease forwards;
}

@keyframes styles-module__connectorOut___5QoPl {
  from {
    opacity: 1;
  }
  to {
    opacity: 0;
  }
}
@keyframes styles-module__connectorDotOut___FEq7e {
  from {
    transform: scale(1);
    opacity: 1;
  }
  to {
    transform: scale(0);
    opacity: 0;
  }
}
@keyframes styles-module__placementEnter___TdRhf {
  from {
    opacity: 0;
    transform: scale(0.85);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
@keyframes styles-module__sectionEnter___-8BXT {
  from {
    opacity: 0;
    transform: scale(0.96);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
@keyframes styles-module__highlightFadeIn___Lg7KY {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
@keyframes styles-module__overlayFadeIn___aECVy {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
@keyframes styles-module__ghostEnter___EC3Mb {
  from {
    opacity: 0;
    transform: scale(0.96);
  }
  to {
    opacity: 0.6;
    transform: scale(1);
  }
}
.styles-module__canvasToggle___-QqSy:focus-visible,
.styles-module__paletteItem___6TlnA:focus-visible {
  outline: 2px solid var(--agentation-color-accent);
  outline-offset: -2px;
}`,$={overlay:"styles-module__overlay___aWh-q",rearrangeOverlay:"styles-module__rearrangeOverlay___-3R3t",overlayExiting:"styles-module__overlayExiting___iEmYr",overlayFadeIn:"styles-module__overlayFadeIn___aECVy",light:"styles-module__light___ORIft",wireframe:"styles-module__wireframe___itvQU",placing:"styles-module__placing___45yD8",passthrough:"styles-module__passthrough___xaFeE",blankCanvas:"styles-module__blankCanvas___t2Eue",visible:"styles-module__visible___OKKqX",gridActive:"styles-module__gridActive___OZ-cf",paletteHeader:"styles-module__paletteHeader___-Q5gQ",paletteHeaderTitle:"styles-module__paletteHeaderTitle___oHqZC",paletteHeaderDesc:"styles-module__paletteHeaderDesc___6i74T",wireframePurposeWrap:"styles-module__wireframePurposeWrap___To-tS",collapsed:"styles-module__collapsed___Ms9vS",wireframePurposeInner:"styles-module__wireframePurposeInner___Lrahs",wireframePurposeInput:"styles-module__wireframePurposeInput___7EtBN",canvasToggle:"styles-module__canvasToggle___-QqSy",active:"styles-module__active___hosp7",canvasToggleIcon:"styles-module__canvasToggleIcon___7pJ82",canvasToggleLabel:"styles-module__canvasToggleLabel___OanpY",placement:"styles-module__placement___zcxv8",placementEnter:"styles-module__placementEnter___TdRhf",selected:"styles-module__selected___6yrp6",dragging:"styles-module__dragging___le6KZ",exiting:"styles-module__exiting___YrM8F",placementContent:"styles-module__placementContent___f64A4",placementLabel:"styles-module__placementLabel___0KvWl",placementAnnotation:"styles-module__placementAnnotation___78pTr",annotationVisible:"styles-module__annotationVisible___mrUyA",sectionAnnotation:"styles-module__sectionAnnotation___aUIs0",handle:"styles-module__handle___Ikbxm",sectionOutline:"styles-module__sectionOutline___s0hy-",ghostOutline:"styles-module__ghostOutline___po-kO",handleNw:"styles-module__handleNw___4TMIj",handleNe:"styles-module__handleNe___mnsTh",handleSe:"styles-module__handleSe___oSFnk",handleSw:"styles-module__handleSw___pi--Z",handleN:"styles-module__handleN___aBA-Q",handleE:"styles-module__handleE___0hM5u",handleS:"styles-module__handleS___JjDRv",handleW:"styles-module__handleW___ERWGQ",edgeHandle:"styles-module__edgeHandle___XxXdT",edgeN:"styles-module__edgeN___-JJDj",edgeS:"styles-module__edgeS___66lMX",edgeE:"styles-module__edgeE___1bGDa",edgeW:"styles-module__edgeW___lHQNo",deleteButton:"styles-module__deleteButton___LkGCb",drawBox:"styles-module__drawBox___BrVAa",selectBox:"styles-module__selectBox___Iu8kB",sizeIndicator:"styles-module__sizeIndicator___7zJ4y",guideLine:"styles-module__guideLine___DUQY2",dragPreview:"styles-module__dragPreview___onPbU",dragPreviewWireframe:"styles-module__dragPreviewWireframe___jsg0G",palette:"styles-module__palette___C7iSH",paletteItem:"styles-module__paletteItem___6TlnA",paletteItemLabel:"styles-module__paletteItemLabel___6ncO4",paletteSectionTitle:"styles-module__paletteSectionTitle___PqnjX",paletteFooter:"styles-module__paletteFooter___QYnAG",paletteSection:"styles-module__paletteSection___V8DEA",paletteItemIcon:"styles-module__paletteItemIcon___0NPQK",placeScroll:"styles-module__placeScroll___7sClM",fadeTop:"styles-module__fadeTop___KT9tF",fadeBottom:"styles-module__fadeBottom___x3ShT",paletteFooterWrap:"styles-module__paletteFooterWrap___71-fI",footerHidden:"styles-module__footerHidden___fJUik",paletteFooterInnerContent:"styles-module__paletteFooterInnerContent___VC26h",paletteFooterInner:"styles-module__paletteFooterInner___dfylY",paletteFooterCount:"styles-module__paletteFooterCount___D3Fia",paletteFooterClear:"styles-module__paletteFooterClear___ybBoa",paletteFooterActions:"styles-module__paletteFooterActions___fLzv8",rollingWrap:"styles-module__rollingWrap___S75jM",rollingNum:"styles-module__rollingNum___1RKDx",exitUp:"styles-module__exitUp___AFDRW",numExitUp:"styles-module__numExitUp___FRQqx",enterUp:"styles-module__enterUp___CPlXb",numEnterUp:"styles-module__numEnterUp___2Yd-w",exitDown:"styles-module__exitDown___-1yAy",numExitDown:"styles-module__numExitDown___xm5by",enterDown:"styles-module__enterDown___DDuFR",numEnterDown:"styles-module__numEnterDown___hpxBk",hoverHighlight:"styles-module__hoverHighlight___8eT-v",highlightFadeIn:"styles-module__highlightFadeIn___Lg7KY",sectionEnter:"styles-module__sectionEnter___-8BXT",settled:"styles-module__settled___b5U5o",sectionLabel:"styles-module__sectionLabel___F80HQ",movedBadge:"styles-module__movedBadge___s8z-q",sectionDimensions:"styles-module__sectionDimensions___RcJSL",badgeVisible:"styles-module__badgeVisible___npbdS",resizedBadge:"styles-module__resizedBadge___u51V8",wireframeNotice:"styles-module__wireframeNotice___4GJyB",wireframeOpacityRow:"styles-module__wireframeOpacityRow___CJXzi",wireframeOpacityLabel:"styles-module__wireframeOpacityLabel___afkfT",wireframeOpacitySlider:"styles-module__wireframeOpacitySlider___YcoEs",wireframeNoticeTitleRow:"styles-module__wireframeNoticeTitleRow___PJqyG",wireframeNoticeTitle:"styles-module__wireframeNoticeTitle___okr08",wireframeNoticeDivider:"styles-module__wireframeNoticeDivider___PNKQ6",wireframeStartOver:"styles-module__wireframeStartOver___YFk-I",ghostEnter:"styles-module__ghostEnter___EC3Mb",ghostBadge:"styles-module__ghostBadge___tsQUK",badgeSlideIn:"styles-module__badgeSlideIn___typJ7",ghostBadgeExtra:"styles-module__ghostBadgeExtra___6CVoD",badgeExtraIn:"styles-module__badgeExtraIn___i4W8F",originalOutline:"styles-module__originalOutline___Y6DD1",originalLabel:"styles-module__originalLabel___HqI9g",connectorSvg:"styles-module__connectorSvg___Lovld",connectorLine:"styles-module__connectorLine___XeWh-",connectorDraw:"styles-module__connectorDraw___8sK5I",connectorDot:"styles-module__connectorDot___yvf7C",connectorDotIn:"styles-module__connectorDotIn___NwTUq",connectorExiting:"styles-module__connectorExiting___2lLOs",connectorOut:"styles-module__connectorOut___5QoPl",connectorDotOut:"styles-module__connectorDotOut___FEq7e"};var ht=ce(ze()),ls=24,Yc=5;function gy(e,t,n,o,r){let i=1/0,s=1/0,l=e.x,a=e.x+e.width,c=e.x+e.width/2,d=e.y,m=e.y+e.height,f=e.y+e.height/2,k=!o,w=k?[l,a,c]:[...o.left?[l]:[],...o.right?[a]:[]],C=k?[d,m,f]:[...o.top?[d]:[],...o.bottom?[m]:[]],S=[];for(let O of t)n.has(O.id)||S.push(O);r&&S.push(...r);for(let O of S){let Ae=O.x,ue=O.x+O.width,J=O.x+O.width/2,Ce=O.y,K=O.y+O.height,Ge=O.y+O.height/2;for(let Le of w)for(let Ye of[Ae,ue,J]){let xe=Ye-Le;Math.abs(xe)<Yc&&Math.abs(xe)<Math.abs(i)&&(i=xe)}for(let Le of C)for(let Ye of[Ce,K,Ge]){let xe=Ye-Le;Math.abs(xe)<Yc&&Math.abs(xe)<Math.abs(s)&&(s=xe)}}let g=Math.abs(i)<Yc?i:0,x=Math.abs(s)<Yc?s:0,h=[],E=new Set,F=l+g,W=a+g,D=c+g,j=d+x,Z=m+x,q=f+x;for(let O of S){let Ae=O.x,ue=O.x+O.width,J=O.x+O.width/2,Ce=O.y,K=O.y+O.height,Ge=O.y+O.height/2;for(let Le of[Ae,J,ue])for(let Ye of[F,D,W])if(Math.abs(Ye-Le)<.5){let xe=`x:${Math.round(Le)}`;E.has(xe)||(E.add(xe),h.push({axis:"x",pos:Le}))}for(let Le of[Ce,Ge,K])for(let Ye of[j,q,Z])if(Math.abs(Ye-Le)<.5){let xe=`y:${Math.round(Le)}`;E.has(xe)||(E.add(xe),h.push({axis:"y",pos:Le}))}}return{dx:g,dy:x,guides:h}}function hy(){return`dp-${Date.now()}-${Math.random().toString(36).slice(2,7)}`}function yy({placements:e,onChange:t,activeComponent:n,onActiveComponentChange:o,isDarkMode:r,exiting:i,onInteractionChange:s,className:l,passthrough:a,extraSnapRects:c,onSelectionChange:d,deselectSignal:m,onDragMove:f,onDragEnd:k,clearingPlacements:w,wireframe:C}){let[S,g]=(0,ot.useState)(new Set),[x,h]=(0,ot.useState)(null),[E,F]=(0,ot.useState)(null),[W,D]=(0,ot.useState)(null),[j,Z]=(0,ot.useState)([]),[q,O]=(0,ot.useState)(null),[Ae,ue]=(0,ot.useState)(!1),J=(0,ot.useRef)(!1),[Ce,K]=(0,ot.useState)(new Set),Ge=(0,ot.useRef)(new Map),Le=(0,ot.useRef)(null),Ye=(0,ot.useRef)(null),xe=(0,ot.useRef)(e);xe.current=e;let gt=(0,ot.useRef)(d);gt.current=d;let Ft=(0,ot.useRef)(f);Ft.current=f;let Kn=(0,ot.useRef)(k);Kn.current=k;let _n=(0,ot.useRef)(m);(0,ot.useEffect)(()=>{m!==_n.current&&(_n.current=m,g(new Set))},[m]),(0,ot.useEffect)(()=>{w?.length&&(g(G=>new Set([...G].filter(_e=>!w.some(Ee=>Ee.id===_e)))),Ye.current=null)},[w]),(0,ot.useEffect)(()=>{let G=_e=>{let Ee=_e.composedPath()[0]||_e.target;if(!(Ee.tagName==="INPUT"||Ee.tagName==="TEXTAREA"||Ee.isContentEditable)){if((_e.key==="Backspace"||_e.key==="Delete")&&S.size>0){_e.preventDefault();let ke=new Set(S);K(ke),g(new Set),De(()=>{t(xe.current.filter(dt=>!ke.has(dt.id))),K(new Set)},180);return}if(["ArrowUp","ArrowDown","ArrowLeft","ArrowRight"].includes(_e.key)&&S.size>0){_e.preventDefault();let ke=_e.shiftKey?20:1,dt=_e.key==="ArrowLeft"?-ke:_e.key==="ArrowRight"?ke:0,rt=_e.key==="ArrowUp"?-ke:_e.key==="ArrowDown"?ke:0;t(e.map(Fe=>S.has(Fe.id)?{...Fe,x:Math.max(0,Fe.x+dt),y:Math.max(0,Fe.y+rt)}:Fe));return}if(_e.key==="Escape"){n?o(null):S.size>0&&g(new Set);return}}};return document.addEventListener("keydown",G),()=>document.removeEventListener("keydown",G)},[S,n,e,t,o]);let Jn=(0,ot.useCallback)(G=>{if(G.button!==0||a||G.target.closest(`.${$.placement}`))return;G.preventDefault(),G.stopPropagation();let Ee=window.scrollY,Pe=G.clientX,ke=G.clientY;if(n){Ye.current="place",s?.(!0);let dt=!1,rt=Pe,Fe=ke,$e=R=>{rt=R.clientX,Fe=R.clientY;let M=Math.abs(rt-Pe),B=Math.abs(Fe-ke);if((M>5||B>5)&&(dt=!0),dt){let U=Math.min(Pe,rt),re=Math.min(ke,Fe),ae=Math.abs(rt-Pe),Y=Math.abs(Fe-ke);h({x:U,y:re,w:ae,h:Y}),D({x:R.clientX+12,y:R.clientY+12,text:`${Math.round(ae)} \xD7 ${Math.round(Y)}`})}},Et=R=>{window.removeEventListener("mousemove",$e),window.removeEventListener("mouseup",Et),h(null),D(null),Ye.current=null,s?.(!1);let M=te[n],B,U,re,ae;dt?(B=Math.min(Pe,rt),U=Math.min(ke,Fe)+Ee,re=Math.max(ls,Math.abs(rt-Pe)),ae=Math.max(ls,Math.abs(Fe-ke))):(re=M.width,ae=M.height,B=Pe-re/2,U=ke+Ee-ae/2),B=Math.max(0,B),U=Math.max(0,U);let Y={id:hy(),type:n,x:B,y:U,width:re,height:ae,scrollY:Ee,timestamp:Date.now()},ie=[...e,Y];t(ie),g(new Set([Y.id])),o(null)};window.addEventListener("mousemove",$e),window.addEventListener("mouseup",Et)}else{G.shiftKey||g(new Set),Ye.current="select";let dt=!1,rt=$e=>{let Et=Math.abs($e.clientX-Pe),R=Math.abs($e.clientY-ke);if((Et>4||R>4)&&(dt=!0),dt){let M=Math.min(Pe,$e.clientX),B=Math.min(ke,$e.clientY);F({x:M,y:B,w:Math.abs($e.clientX-Pe),h:Math.abs($e.clientY-ke)})}},Fe=$e=>{if(window.removeEventListener("mousemove",rt),window.removeEventListener("mouseup",Fe),Ye.current=null,dt){let Et=Math.min(Pe,$e.clientX),R=Math.min(ke,$e.clientY)+Ee,M=Math.abs($e.clientX-Pe),B=Math.abs($e.clientY-ke),U=new Set(G.shiftKey?S:new Set);for(let re of e){let ae=re.y-Ee;re.x+re.width>Et&&re.x<Et+M&&re.y+re.height>R&&re.y<R+B&&U.add(re.id)}g(U)}F(null)};window.addEventListener("mousemove",rt),window.addEventListener("mouseup",Fe)}},[n,a,e,t,S]),Ve=(0,ot.useCallback)((G,_e)=>{if(G.button!==0)return;let Ee=G.target;if(Ee.closest(`.${$.handle}`)||Ee.closest(`.${$.deleteButton}`))return;G.preventDefault(),G.stopPropagation();let Pe;G.shiftKey?(Pe=new Set(S),Pe.has(_e)?Pe.delete(_e):Pe.add(_e)):S.has(_e)?Pe=new Set(S):Pe=new Set([_e]),g(Pe),(Pe.size!==S.size||[...Pe].some(ie=>!S.has(ie)))&&gt.current?.(Pe,G.shiftKey);let dt=window.scrollY,rt=G.clientX,Fe=G.clientY,$e=new Map;for(let ie of e)Pe.has(ie.id)&&$e.set(ie.id,{x:ie.x,y:ie.y});Ye.current="move",s?.(!0);let Et=!1,R=!1,M=e,B=0,U=0,re=new Map;for(let ie of e)$e.has(ie.id)&&re.set(ie.id,{w:ie.width,h:ie.height});let ae=ie=>{let ve=ie.clientX-rt,Oe=ie.clientY-Fe;if((Math.abs(ve)>2||Math.abs(Oe)>2)&&(Et=!0),!Et)return;if(ie.altKey&&!R){R=!0;let lt=[];for(let yt of e)$e.has(yt.id)&&lt.push({...yt,id:hy(),timestamp:Date.now()});M=[...e,...lt]}let it=1/0,Ze=1/0,_t=-1/0,je=-1/0;for(let[lt,yt]of $e){let Yt=re.get(lt);Yt&&(it=Math.min(it,yt.x+ve),Ze=Math.min(Ze,yt.y+Oe),_t=Math.max(_t,yt.x+ve+Yt.w),je=Math.max(je,yt.y+Oe+Yt.h))}let Se={x:it,y:Ze,width:_t-it,height:je-Ze},{dx:ut,dy:Mt,guides:st}=gy(Se,M,new Set($e.keys()),void 0,c);Z(st);let Ne=ve+ut,et=Oe+Mt;B=Ne,U=et,t(M.map(lt=>{let yt=$e.get(lt.id);return yt?{...lt,x:Math.max(0,yt.x+Ne),y:Math.max(0,yt.y+et)}:lt})),Ft.current?.(Ne,et)},Y=()=>{window.removeEventListener("mousemove",ae),window.removeEventListener("mouseup",Y),Ye.current=null,s?.(!1),Z([]),Kn.current?.(B,U,Et)};window.addEventListener("mousemove",ae),window.addEventListener("mouseup",Y)},[S,e,t,s]),vn=(0,ot.useCallback)((G,_e,Ee)=>{G.preventDefault(),G.stopPropagation();let Pe=e.find(U=>U.id===_e);if(!Pe)return;g(new Set([_e])),Ye.current="resize",s?.(!0);let ke=G.clientX,dt=G.clientY,rt=Pe.width,Fe=Pe.height,$e=Pe.x,Et=Pe.y,R={left:Ee.includes("w"),right:Ee.includes("e"),top:Ee.includes("n"),bottom:Ee.includes("s")},M=U=>{let re=U.clientX-ke,ae=U.clientY-dt,Y=rt,ie=Fe,ve=$e,Oe=Et;Ee.includes("e")&&(Y=Math.max(ls,rt+re)),Ee.includes("w")&&(Y=Math.max(ls,rt-re),ve=$e+rt-Y),Ee.includes("s")&&(ie=Math.max(ls,Fe+ae)),Ee.includes("n")&&(ie=Math.max(ls,Fe-ae),Oe=Et+Fe-ie);let it={x:ve,y:Oe,width:Y,height:ie},{dx:Ze,dy:_t,guides:je}=gy(it,xe.current,new Set([_e]),R,c);Z(je),Ze!==0&&(R.right?Y+=Ze:R.left&&(ve+=Ze,Y-=Ze)),_t!==0&&(R.bottom?ie+=_t:R.top&&(Oe+=_t,ie-=_t)),t(xe.current.map(Se=>Se.id===_e?{...Se,x:ve,y:Oe,width:Y,height:ie}:Se)),D({x:U.clientX+12,y:U.clientY+12,text:`${Math.round(Y)} \xD7 ${Math.round(ie)}`})},B=()=>{window.removeEventListener("mousemove",M),window.removeEventListener("mouseup",B),D(null),Ye.current=null,s?.(!1),Z([])};window.addEventListener("mousemove",M),window.addEventListener("mouseup",B)},[e,t,s]),po=(0,ot.useCallback)(G=>{Ye.current=null,K(_e=>{let Ee=new Set(_e);return Ee.add(G),Ee}),g(_e=>{let Ee=new Set(_e);return Ee.delete(G),Ee}),De(()=>{t(xe.current.filter(_e=>_e.id!==G)),K(_e=>{let Ee=new Set(_e);return Ee.delete(G),Ee})},180)},[t]),fo=new Set(["text","hero","button","badge","cta","toast","modal","card","navigation","tabs","input","search","breadcrumb","pricing","testimonial","alert","banner","tag","notification","stat","productCard"]),Qo={hero:"Headline text",button:"Button label",badge:"Badge label",cta:"Call to action text",toast:"Notification message",modal:"Dialog title",card:"Card title",navigation:"Brand / nav items",tabs:"Tab labels",input:"Placeholder text",search:"Search placeholder",pricing:"Plan name or price",testimonial:"Quote text",alert:"Alert message",banner:"Banner text",tag:"Tag label",notification:"Notification message",stat:"Metric value",productCard:"Product name"},Go=(0,ot.useCallback)(G=>{let _e=e.find(Ee=>Ee.id===G);_e&&(J.current=!!_e.text,O(G),ue(!1))},[e]),On=(0,ot.useCallback)(()=>{q&&(ue(!0),De(()=>{O(null),ue(!1)},150))},[q]);(0,ot.useEffect)(()=>{i&&q&&On()},[i]);let Er=(0,ot.useCallback)(G=>{q&&(t(e.map(_e=>_e.id===q?{..._e,text:G.trim()||void 0}:_e)),On())},[q,e,t,On]),Ot=typeof window<"u"?window.scrollY:0,Zn=["nw","ne","se","sw"],go=C?"#f97316":"#3c82f7",Mr=[{dir:"n",cls:$.edgeN,arrow:(0,ht.jsx)("svg",{width:"8",height:"6",viewBox:"0 0 8 6",fill:"none",children:(0,ht.jsx)("path",{d:"M4 0.5L1 4.5h6z",fill:go})})},{dir:"e",cls:$.edgeE,arrow:(0,ht.jsx)("svg",{width:"6",height:"8",viewBox:"0 0 6 8",fill:"none",children:(0,ht.jsx)("path",{d:"M5.5 4L1.5 1v6z",fill:go})})},{dir:"s",cls:$.edgeS,arrow:(0,ht.jsx)("svg",{width:"8",height:"6",viewBox:"0 0 8 6",fill:"none",children:(0,ht.jsx)("path",{d:"M4 5.5L1 1.5h6z",fill:go})})},{dir:"w",cls:$.edgeW,arrow:(0,ht.jsx)("svg",{width:"6",height:"8",viewBox:"0 0 6 8",fill:"none",children:(0,ht.jsx)("path",{d:"M0.5 4L4.5 1v6z",fill:go})})}];return(0,ht.jsxs)(ht.Fragment,{children:[(0,ht.jsx)("div",{ref:Le,className:`${$.overlay} ${r?"":$.light} ${n?$.placing:""} ${a?$.passthrough:""} ${i?$.overlayExiting:""} ${C?$.wireframe:""}${l?` ${l}`:""}`,"data-feedback-toolbar":!0,onMouseDown:Jn,children:e.map(G=>{let _e=S.has(G.id),Ee=Fn[G.type]?.label||G.type,Pe=G.y-Ot;return(0,ht.jsxs)("div",{"data-design-placement":G.id,className:`${$.placement} ${_e?$.selected:""} ${Ce.has(G.id)||w?.includes(G)?$.exiting:""}`,style:{left:G.x,top:Pe,width:G.width,height:G.height,position:"fixed"},onMouseDown:ke=>Ve(ke,G.id),onDoubleClick:()=>Go(G.id),children:[(0,ht.jsx)("span",{className:$.placementLabel,children:Ee}),(0,ht.jsx)("span",{className:`${$.placementAnnotation} ${G.text?$.annotationVisible:""}`,children:(G.text&&Ge.current.set(G.id,G.text),G.text||Ge.current.get(G.id)||"")}),(0,ht.jsx)("div",{className:$.placementContent,children:(0,ht.jsx)(py,{type:G.type,width:G.width,height:G.height,text:G.text})}),(0,ht.jsx)("div",{className:$.deleteButton,onMouseDown:ke=>ke.stopPropagation(),onClick:()=>po(G.id),children:"\u2715"}),Zn.map(ke=>(0,ht.jsx)("div",{className:`${$.handle} ${$[`handle${ke.charAt(0).toUpperCase()}${ke.slice(1)}`]}`,onMouseDown:dt=>vn(dt,G.id,ke)},ke)),Mr.map(({dir:ke,cls:dt,arrow:rt})=>(0,ht.jsx)("div",{className:`${$.edgeHandle} ${dt}`,onMouseDown:Fe=>vn(Fe,G.id,ke),children:rt},ke))]},G.id)})}),q&&(()=>{let G=e.find(Et=>Et.id===q);if(!G)return null;let _e=G.y-Ot,Ee=G.x+G.width/2,Pe=_e-8,ke=_e+G.height+8,dt=Pe>200,rt=ke<window.innerHeight-100,Fe=Math.max(160,Math.min(window.innerWidth-160,Ee)),$e;return dt?$e={left:Fe,bottom:window.innerHeight-Pe}:rt?$e={left:Fe,top:ke}:$e={left:Fe,top:Math.max(80,window.innerHeight/2-80)},(0,ht.jsx)(is,{element:Fn[G.type]?.label||G.type,placeholder:Qo[G.type]||"Label or content text",initialValue:G.text??"",submitLabel:J.current?"Save":"Set",onSubmit:Er,onCancel:On,onDelete:J.current?()=>{Er("")}:void 0,isExiting:Ae,lightMode:!r,style:$e})})(),x&&(0,ht.jsx)("div",{className:$.drawBox,style:{left:x.x,top:x.y,width:x.w,height:x.h},"data-feedback-toolbar":!0}),E&&(0,ht.jsx)("div",{className:$.selectBox,style:{left:E.x,top:E.y,width:E.w,height:E.h},"data-feedback-toolbar":!0}),W&&(0,ht.jsx)("div",{className:$.sizeIndicator,style:{left:W.x,top:W.y},"data-feedback-toolbar":!0,children:W.text}),j.map((G,_e)=>(0,ht.jsx)("div",{className:$.guideLine,style:G.axis==="x"?{position:"fixed",left:G.pos,top:0,width:1,bottom:0}:{position:"fixed",left:0,top:G.pos-Ot,right:0,height:1},"data-feedback-toolbar":!0},`${G.axis}-${G.pos}-${_e}`))]})}var Gt=ce(vt());var wr=ce(vt());function Vc(e,{keepMounted:t=!1,onExited:n}={}){let[o,r]=(0,wr.useState)(t||e),i=(0,wr.useRef)(null),s=(0,wr.useRef)(n);return e&&!o&&r(!0),(0,wr.useLayoutEffect)(()=>{s.current=n},[n]),(0,wr.useLayoutEffect)(()=>{let l=i.current;if(!l||l.dataset.panelOpen==="true"===e)return;getComputedStyle(l).opacity,l.dataset.panelPresent="true",l.dataset.panelOpen=String(e);let a=!1,c=l.getAnimations?.()??[];return Promise.allSettled(c.map(d=>d.finished)).then(()=>{a||e||(delete l.dataset.panelPresent,t||r(!1),s.current?.())}),()=>{a=!0}},[e,o,t]),{ref:i,mounted:o}}var u=ce(ze());function Cw(e){if(!e)return"";let t=e.scrollTop>2,n=e.scrollTop+e.clientHeight<e.scrollHeight-2;return`${t?$.fadeTop:""} ${n?$.fadeBottom:""}`}var v="currentColor",A="0.5";function Ew({type:e}){switch(e){case"navigation":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"4",width:"18",height:"8",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"2.5",y:"7",width:"3",height:"1.5",rx:".5",fill:v,opacity:".4"}),(0,u.jsx)("rect",{x:"7",y:"7",width:"2.5",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"11",y:"7",width:"2.5",height:"1.5",rx:".5",fill:v,opacity:".25"})]});case"header":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"2",width:"18",height:"12",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"3",y:"5.5",width:"8",height:"2",rx:".5",fill:v,opacity:".35"}),(0,u.jsx)("rect",{x:"3",y:"9",width:"12",height:"1",rx:".5",fill:v,opacity:".15"})]});case"hero":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"1",width:"18",height:"14",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"5",y:"5",width:"10",height:"1.5",rx:".5",fill:v,opacity:".35"}),(0,u.jsx)("rect",{x:"7",y:"8",width:"6",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"7.5",y:"10.5",width:"5",height:"2.5",rx:"1",stroke:v,strokeWidth:A})]});case"section":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"1",width:"18",height:"14",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"3",y:"4",width:"6",height:"1",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"3",y:"6.5",width:"14",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"3",y:"9",width:"10",height:"1",rx:".5",fill:v,opacity:".15"})]});case"sidebar":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"1",width:"7",height:"14",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"2.5",y:"4",width:"4",height:"1",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"2.5",y:"6.5",width:"3.5",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"2.5",y:"9",width:"4",height:"1",rx:".5",fill:v,opacity:".15"})]});case"footer":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"7",width:"18",height:"8",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"3",y:"9.5",width:"4",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"9.5",width:"4",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"15",y:"9.5",width:"3",height:"1",rx:".5",fill:v,opacity:".2"})]});case"modal":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"12",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"5",y:"4.5",width:"7",height:"1",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"5",y:"7",width:"10",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"11",y:"11",width:"5",height:"2",rx:".75",stroke:v,strokeWidth:A})]});case"divider":return(0,u.jsx)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:(0,u.jsx)("line",{x1:"2",y1:"8",x2:"18",y2:"8",stroke:v,strokeWidth:"0.5",opacity:".3"})});case"card":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"14",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"5.5",rx:"1",fill:v,opacity:".04"}),(0,u.jsx)("rect",{x:"4",y:"8.5",width:"8",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"4",y:"11",width:"11",height:"1",rx:".5",fill:v,opacity:".12"})]});case"text":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4",width:"14",height:"1.5",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"2",y:"7",width:"11",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"2",y:"9.5",width:"13",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"2",y:"12",width:"8",height:"1",rx:".5",fill:v,opacity:".12"})]});case"image":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("line",{x1:"2",y1:"2",x2:"18",y2:"14",stroke:v,strokeWidth:".3",opacity:".25"}),(0,u.jsx)("line",{x1:"18",y1:"2",x2:"2",y2:"14",stroke:v,strokeWidth:".3",opacity:".25"})]});case"video":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("path",{d:"M8.5 5.5v5l4.5-2.5z",stroke:v,strokeWidth:A,fill:v,opacity:".15"})]});case"table":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"2",width:"18",height:"12",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("line",{x1:"1",y1:"5.5",x2:"19",y2:"5.5",stroke:v,strokeWidth:".3",opacity:".25"}),(0,u.jsx)("line",{x1:"1",y1:"9",x2:"19",y2:"9",stroke:v,strokeWidth:".3",opacity:".25"}),(0,u.jsx)("line",{x1:"7",y1:"2",x2:"7",y2:"14",stroke:v,strokeWidth:".3",opacity:".25"}),(0,u.jsx)("line",{x1:"13",y1:"2",x2:"13",y2:"14",stroke:v,strokeWidth:".3",opacity:".25"})]});case"grid":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"2",width:"7",height:"5.5",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"11.5",y:"2",width:"7",height:"5.5",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"1.5",y:"9.5",width:"7",height:"5.5",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"11.5",y:"9.5",width:"7",height:"5.5",rx:"1",stroke:v,strokeWidth:A})]});case"list":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"3.5",cy:"4.5",r:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6.5",y:"4",width:"10",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"3.5",cy:"8",r:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6.5",y:"7.5",width:"8",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"3.5",cy:"11.5",r:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6.5",y:"11",width:"11",height:"1",rx:".5",fill:v,opacity:".2"})]});case"chart":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"9",width:"2.5",height:"4",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"7",y:"6",width:"2.5",height:"7",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"11",y:"3",width:"2.5",height:"10",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"15",y:"5",width:"2.5",height:"8",rx:".5",fill:v,opacity:".2"})]});case"accordion":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"2",width:"17",height:"4",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"3",y:"3.5",width:"6",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"1.5",y:"7.5",width:"17",height:"3",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"1.5",y:"12",width:"17",height:"3",rx:"1",stroke:v,strokeWidth:A})]});case"carousel":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"10",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("path",{d:"M1.5 7L3 8.5 1.5 10",stroke:v,strokeWidth:A,opacity:".35"}),(0,u.jsx)("path",{d:"M18.5 7L17 8.5 18.5 10",stroke:v,strokeWidth:A,opacity:".35"}),(0,u.jsx)("circle",{cx:"8.5",cy:"14",r:".6",fill:v,opacity:".35"}),(0,u.jsx)("circle",{cx:"10",cy:"14",r:".6",fill:v,opacity:".15"}),(0,u.jsx)("circle",{cx:"11.5",cy:"14",r:".6",fill:v,opacity:".15"})]});case"button":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"5",width:"14",height:"6",rx:"2",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6.5",y:"7.5",width:"7",height:"1",rx:".5",fill:v,opacity:".25"})]});case"input":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4",width:"5.5",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"2",y:"6.5",width:"16",height:"5.5",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"3.5",y:"8.5",width:"7",height:"1",rx:".5",fill:v,opacity:".12"})]});case"search":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4.5",width:"16",height:"7",rx:"3.5",stroke:v,strokeWidth:A}),(0,u.jsx)("circle",{cx:"6",cy:"8",r:"2",stroke:v,strokeWidth:A,opacity:".3"}),(0,u.jsx)("line",{x1:"7.5",y1:"9.5",x2:"9",y2:"11",stroke:v,strokeWidth:A,opacity:".3"}),(0,u.jsx)("rect",{x:"9.5",y:"7.5",width:"6",height:"1",rx:".5",fill:v,opacity:".12"})]});case"form":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1.5",width:"5.5",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"2",y:"3.5",width:"16",height:"3",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"2",y:"8",width:"7",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"2",y:"10",width:"16",height:"3",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"12",y:"14",width:"6",height:"2",rx:".75",stroke:v,strokeWidth:A})]});case"tabs":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"5",width:"18",height:"10",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"1",y:"2",width:"6",height:"3.5",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"2.5",y:"3.25",width:"3",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"2",width:"6",height:"3.5",rx:".75",stroke:v,strokeWidth:A})]});case"dropdown":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"4",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"3.5",y:"3.5",width:"7",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("path",{d:"M15 3.5l1.5 1.5L18 3.5",stroke:v,strokeWidth:A,opacity:".3"}),(0,u.jsx)("rect",{x:"2",y:"7",width:"16",height:"7",rx:"1",stroke:v,strokeWidth:A,strokeDasharray:"2 1",opacity:".3"})]});case"toggle":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"4",y:"5",width:"12",height:"6",rx:"3",stroke:v,strokeWidth:A}),(0,u.jsx)("circle",{cx:"13",cy:"8",r:"2",fill:v,opacity:".3"})]});case"avatar":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"10",cy:"8",r:"6",stroke:v,strokeWidth:A}),(0,u.jsx)("circle",{cx:"10",cy:"6.5",r:"2",stroke:v,strokeWidth:A}),(0,u.jsx)("path",{d:"M6.5 13c0-2 1.5-3.5 3.5-3.5s3.5 1.5 3.5 3.5",stroke:v,strokeWidth:A})]});case"badge":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"5",width:"14",height:"6",rx:"3",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6",y:"7.5",width:"8",height:"1",rx:".5",fill:v,opacity:".25"})]});case"breadcrumb":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"7",width:"3.5",height:"1",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("path",{d:"M6.5 7l1 1-1 1",stroke:v,strokeWidth:A,opacity:".2"}),(0,u.jsx)("rect",{x:"9",y:"7",width:"3.5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("path",{d:"M14 7l1 1-1 1",stroke:v,strokeWidth:A,opacity:".2"}),(0,u.jsx)("rect",{x:"16.5",y:"7",width:"2",height:"1",rx:".5",fill:v,opacity:".15"})]});case"pagination":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"5.5",width:"3.5",height:"5",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6.5",y:"5.5",width:"3.5",height:"5",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"11",y:"5.5",width:"3.5",height:"5",rx:"1",fill:v,opacity:".15",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"15.5",y:"5.5",width:"3.5",height:"5",rx:"1",stroke:v,strokeWidth:A})]});case"progress":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"7",width:"16",height:"2",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"2",y:"7",width:"10",height:"2",rx:"1",fill:v,opacity:".2"})]});case"toast":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4",width:"16",height:"8",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("circle",{cx:"5",cy:"8",r:"1.5",stroke:v,strokeWidth:A,opacity:".3"}),(0,u.jsx)("rect",{x:"8",y:"6.5",width:"7",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"8",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".12"})]});case"tooltip":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"3",width:"14",height:"7",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"5.5",y:"5.5",width:"9",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("path",{d:"M9 10l1 2.5 1-2.5",stroke:v,strokeWidth:A})]});case"pricing":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"14",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6",y:"3",width:"8",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"5.5",width:"6",height:"2",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"5",y:"9",width:"10",height:"1",rx:".5",fill:v,opacity:".1"}),(0,u.jsx)("rect",{x:"5",y:"11",width:"10",height:"1",rx:".5",fill:v,opacity:".1"}),(0,u.jsx)("rect",{x:"6",y:"13",width:"8",height:"1.5",rx:".5",fill:v,opacity:".2"})]});case"testimonial":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"14",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("text",{x:"4",y:"5.5",fontSize:"4",fill:v,opacity:".2",fontFamily:"serif",children:"\u201C"}),(0,u.jsx)("rect",{x:"4",y:"7",width:"12",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"4",y:"9",width:"9",height:"1",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("circle",{cx:"5.5",cy:"12.5",r:"1.5",stroke:v,strokeWidth:A,opacity:".25"}),(0,u.jsx)("rect",{x:"8",y:"12",width:"5",height:"1",rx:".5",fill:v,opacity:".15"})]});case"cta":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"2",width:"18",height:"12",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"5",y:"4.5",width:"10",height:"1.5",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"6",y:"7.5",width:"8",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"7",y:"10",width:"6",height:"2.5",rx:"1",stroke:v,strokeWidth:A})]});case"alert":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4",width:"16",height:"8",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("circle",{cx:"6",cy:"8",r:"2",stroke:v,strokeWidth:A,opacity:".3"}),(0,u.jsx)("line",{x1:"6",y1:"7",x2:"6",y2:"8.5",stroke:v,strokeWidth:"0.6",opacity:".5"}),(0,u.jsx)("circle",{cx:"6",cy:"9.3",r:".3",fill:v,opacity:".5"}),(0,u.jsx)("rect",{x:"9.5",y:"7",width:"6",height:"1",rx:".5",fill:v,opacity:".2"})]});case"banner":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"5",width:"18",height:"6",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"4",y:"7.5",width:"8",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"14",y:"7",width:"3.5",height:"2",rx:".75",stroke:v,strokeWidth:A})]});case"stat":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"12",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6",y:"4.5",width:"8",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"5",y:"7",width:"10",height:"2.5",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"7",y:"11",width:"6",height:"1",rx:".5",fill:v,opacity:".12"})]});case"stepper":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"4",cy:"8",r:"2",fill:v,opacity:".2",stroke:v,strokeWidth:A}),(0,u.jsx)("line",{x1:"6",y1:"8",x2:"8",y2:"8",stroke:v,strokeWidth:".4",opacity:".3"}),(0,u.jsx)("circle",{cx:"10",cy:"8",r:"2",stroke:v,strokeWidth:A}),(0,u.jsx)("line",{x1:"12",y1:"8",x2:"14",y2:"8",stroke:v,strokeWidth:".4",opacity:".3"}),(0,u.jsx)("circle",{cx:"16",cy:"8",r:"2",stroke:v,strokeWidth:A})]});case"tag":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"5",width:"14",height:"6",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"5.5",y:"7.5",width:"6",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("line",{x1:"14",y1:"6.5",x2:"15.5",y2:"9.5",stroke:v,strokeWidth:A,opacity:".2"}),(0,u.jsx)("line",{x1:"15.5",y1:"6.5",x2:"14",y2:"9.5",stroke:v,strokeWidth:A,opacity:".2"})]});case"rating":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("path",{d:"M4 5.5l1 2 2.2.3-1.6 1.5.4 2.2L4 10.3l-2 1.2.4-2.2L.8 7.8 3 7.5z",fill:v,opacity:".25"}),(0,u.jsx)("path",{d:"M10 5.5l1 2 2.2.3-1.6 1.5.4 2.2L10 10.3l-2 1.2.4-2.2L6.8 7.8 9 7.5z",fill:v,opacity:".25"}),(0,u.jsx)("path",{d:"M16 5.5l1 2 2.2.3-1.6 1.5.4 2.2L16 10.3l-2 1.2.4-2.2-1.6-1.5 2.2-.3z",stroke:v,strokeWidth:A,opacity:".25"})]});case"map":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("line",{x1:"2",y1:"6",x2:"18",y2:"10",stroke:v,strokeWidth:".3",opacity:".15"}),(0,u.jsx)("line",{x1:"7",y1:"2",x2:"11",y2:"14",stroke:v,strokeWidth:".3",opacity:".15"}),(0,u.jsx)("path",{d:"M10 5c-1.7 0-3 1.3-3 3 0 2.5 3 5 3 5s3-2.5 3-5c0-1.7-1.3-3-3-3z",fill:v,opacity:".15",stroke:v,strokeWidth:A})]});case"timeline":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("line",{x1:"5",y1:"2",x2:"5",y2:"14",stroke:v,strokeWidth:".4",opacity:".25"}),(0,u.jsx)("circle",{cx:"5",cy:"4",r:"1.5",fill:v,opacity:".2",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"8",y:"3",width:"8",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("circle",{cx:"5",cy:"8.5",r:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"8",y:"7.5",width:"6",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("circle",{cx:"5",cy:"13",r:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"8",y:"12",width:"7",height:"1",rx:".5",fill:v,opacity:".15"})]});case"fileUpload":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"12",rx:"1.5",stroke:v,strokeWidth:A,strokeDasharray:"2 1"}),(0,u.jsx)("path",{d:"M10 10V5.5m0 0L7.5 8m2.5-2.5L12.5 8",stroke:v,strokeWidth:A,opacity:".3"}),(0,u.jsx)("rect",{x:"7",y:"11.5",width:"6",height:"1",rx:".5",fill:v,opacity:".15"})]});case"codeBlock":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("circle",{cx:"4",cy:"4",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("circle",{cx:"5.5",cy:"4",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("circle",{cx:"7",cy:"4",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"4",y:"7",width:"7",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"6",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"4",y:"11",width:"8",height:"1",rx:".5",fill:v,opacity:".12"})]});case"calendar":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"3",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("line",{x1:"2",y1:"6.5",x2:"18",y2:"6.5",stroke:v,strokeWidth:".4",opacity:".25"}),(0,u.jsx)("rect",{x:"5",y:"4",width:"1",height:"1.5",rx:".3",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"14",y:"4",width:"1",height:"1.5",rx:".3",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"7",cy:"9",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"9",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"13",cy:"9",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("circle",{cx:"7",cy:"12",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"12",r:".6",fill:v,opacity:".2"})]});case"notification":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"3",width:"16",height:"10",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("circle",{cx:"5.5",cy:"8",r:"2",stroke:v,strokeWidth:A,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"6",width:"6",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"8.5",width:"4.5",height:"1",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("circle",{cx:"16.5",cy:"4.5",r:"1.5",fill:v,opacity:".25"})]});case"productCard":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"1",width:"14",height:"14",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"3",y:"1",width:"14",height:"6",rx:"1",fill:v,opacity:".04"}),(0,u.jsx)("rect",{x:"5",y:"8.5",width:"7",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"5",y:"10.5",width:"4",height:"1.5",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"12",y:"12",width:"4",height:"2",rx:".75",stroke:v,strokeWidth:A})]});case"profile":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"10",cy:"5",r:"3",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"5",y:"10",width:"10",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"12.5",width:"6",height:"1",rx:".5",fill:v,opacity:".12"})]});case"drawer":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"9",y:"1",width:"10",height:"14",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"10.5",y:"4",width:"5",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"10.5",y:"6.5",width:"7",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"10.5",y:"9",width:"6",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"1",y:"1",width:"7",height:"14",rx:"1",stroke:v,strokeWidth:A,opacity:".15"})]});case"popover":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"9",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"5",y:"4.5",width:"8",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"5",y:"7",width:"6",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("path",{d:"M9 11l1 2.5 1-2.5",stroke:v,strokeWidth:A})]});case"logo":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"3",width:"10",height:"10",rx:"2",stroke:v,strokeWidth:A}),(0,u.jsx)("path",{d:"M5 9.5l2-4 2 4",stroke:v,strokeWidth:A,opacity:".3"}),(0,u.jsx)("rect",{x:"14",y:"6",width:"4",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"14",y:"8.5",width:"3",height:"1",rx:".5",fill:v,opacity:".12"})]});case"faq":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("text",{x:"2.5",y:"5.5",fontSize:"4",fill:v,opacity:".3",fontWeight:"bold",children:"?"}),(0,u.jsx)("rect",{x:"7",y:"3",width:"10",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"5.5",width:"8",height:"1",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("text",{x:"2.5",y:"11.5",fontSize:"4",fill:v,opacity:".3",fontWeight:"bold",children:"?"}),(0,u.jsx)("rect",{x:"7",y:"9",width:"9",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"11.5",width:"7",height:"1",rx:".5",fill:v,opacity:".12"})]});case"gallery":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"1.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"7.5",y:"1.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"13.5",y:"1.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"1.5",y:"9.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"7.5",y:"9.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"13.5",y:"9.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:A})]});case"checkbox":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"5",y:"4",width:"8",height:"8",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("path",{d:"M7.5 8l1.5 1.5 3-3",stroke:v,strokeWidth:A,opacity:".35"})]});case"radio":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"10",cy:"8",r:"4",stroke:v,strokeWidth:A}),(0,u.jsx)("circle",{cx:"10",cy:"8",r:"2",fill:v,opacity:".3"})]});case"slider":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"7.5",width:"16",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"2",y:"7.5",width:"10",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("circle",{cx:"12",cy:"8",r:"2.5",stroke:v,strokeWidth:A})]});case"datePicker":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"5",rx:"1",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"3.5",y:"3",width:"5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"14",y:"2.5",width:"2.5",height:"2",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("rect",{x:"2",y:"7",width:"16",height:"8",rx:"1",stroke:v,strokeWidth:A,strokeDasharray:"2 1",opacity:".3"}),(0,u.jsx)("circle",{cx:"6",cy:"10",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"10",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("circle",{cx:"14",cy:"10",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"6",cy:"13",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"13",r:".6",fill:v,opacity:".2"})]});case"skeleton":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"3",rx:"1",fill:v,opacity:".08"}),(0,u.jsx)("rect",{x:"2",y:"7",width:"10",height:"2",rx:".75",fill:v,opacity:".08"}),(0,u.jsx)("rect",{x:"2",y:"11",width:"13",height:"2",rx:".75",fill:v,opacity:".08"})]});case"chip":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"5",width:"10",height:"6",rx:"3",fill:v,opacity:".08",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"4",y:"7.5",width:"4",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("line",{x1:"9.5",y1:"6.5",x2:"10.5",y2:"9.5",stroke:v,strokeWidth:A,opacity:".2"}),(0,u.jsx)("line",{x1:"10.5",y1:"6.5",x2:"9.5",y2:"9.5",stroke:v,strokeWidth:A,opacity:".2"}),(0,u.jsx)("rect",{x:"13",y:"5",width:"5.5",height:"6",rx:"3",stroke:v,strokeWidth:A,opacity:".25"})]});case"icon":return(0,u.jsx)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:(0,u.jsx)("path",{d:"M10 3l1.5 3 3.5.5-2.5 2.5.5 3.5L10 11l-3 1.5.5-3.5L5 6.5l3.5-.5z",stroke:v,strokeWidth:A,opacity:".3"})});case"spinner":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"10",cy:"8",r:"5",stroke:v,strokeWidth:A,opacity:".12"}),(0,u.jsx)("path",{d:"M10 3a5 5 0 0 1 5 5",stroke:v,strokeWidth:A,opacity:".35",strokeLinecap:"round"})]});case"feature":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"5",height:"5",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("path",{d:"M4.5 3.5v3m-1.5-1.5h3",stroke:v,strokeWidth:A,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"2.5",width:"8",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"5.5",width:"6",height:"1",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("rect",{x:"2",y:"10",width:"5",height:"5",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"9",y:"10.5",width:"7",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"13.5",width:"5",height:"1",rx:".5",fill:v,opacity:".12"})]});case"team":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"5",cy:"5",r:"2.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"2.5",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"15",cy:"5",r:"2.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"12.5",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"5",r:"2.5",stroke:v,strokeWidth:A,opacity:".5"}),(0,u.jsx)("rect",{x:"7.5",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"4",y:"12",width:"12",height:"1",rx:".5",fill:v,opacity:".1"})]});case"login":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"1",width:"14",height:"14",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6",y:"3",width:"8",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"5",y:"5.5",width:"10",height:"3",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"5",y:"9.5",width:"10",height:"3",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"6.5",y:"13.5",width:"7",height:"2",rx:".75",fill:v,opacity:".2"})]});case"contact":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"14",rx:"1.5",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"4",y:"3",width:"5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"4",y:"5",width:"12",height:"2.5",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"4",y:"8.5",width:"12",height:"4",rx:".75",stroke:v,strokeWidth:A}),(0,u.jsx)("rect",{x:"11",y:"13.5",width:"5",height:"1.5",rx:".5",fill:v,opacity:".2"})]});default:return null}}function Mw({activeType:e,onSelect:t,onDragStart:n,scrollRef:o,fadeClass:r,blankCanvas:i}){return(0,u.jsx)("div",{ref:o,className:`${$.placeScroll} ${r||""}`,children:hm.map(s=>(0,u.jsxs)("div",{className:$.paletteSection,children:[(0,u.jsx)("div",{className:$.paletteSectionTitle,children:s.section}),s.items.map(l=>(0,u.jsxs)("button",{type:"button","aria-pressed":e===l.type,className:`${$.paletteItem} ${e===l.type?$.active:""} ${i?$.wireframe:""}`,onClick:()=>t(l.type),onMouseDown:a=>{a.button===0&&n(l.type,a)},children:[(0,u.jsx)("span",{className:$.paletteItemIcon,"aria-hidden":"true",children:(0,u.jsx)(Ew,{type:l.type})}),(0,u.jsx)("span",{className:$.paletteItemLabel,children:l.label})]},l.type))]},s.section))})}function Tw({value:e,suffix:t}){let[n,o]=(0,Gt.useState)(null),[r,i]=(0,Gt.useState)(t),[s,l]=(0,Gt.useState)("up"),a=(0,Gt.useRef)(e),c=(0,Gt.useRef)(t),d=(0,Gt.useRef)(),m=n!==null&&r!==t;return(0,Gt.useEffect)(()=>{if(e!==a.current){if(e===0){a.current=e,c.current=t,o(null);return}l(e>a.current?"up":"down"),o(a.current),i(c.current),a.current=e,c.current=t,clearTimeout(d.current),d.current=De(()=>o(null),250)}else c.current=t},[e,t]),n===null?(0,u.jsxs)(u.Fragment,{children:[e,t?` ${t}`:""]}):m?(0,u.jsxs)("span",{className:$.rollingWrap,children:[(0,u.jsxs)("span",{style:{visibility:"hidden"},children:[e," ",t]}),(0,u.jsxs)("span",{className:`${$.rollingNum} ${s==="up"?$.exitUp:$.exitDown}`,children:[n," ",r]},`o${n}-${e}`),(0,u.jsxs)("span",{className:`${$.rollingNum} ${s==="up"?$.enterUp:$.enterDown}`,children:[e," ",t]},`n${e}`)]}):(0,u.jsxs)(u.Fragment,{children:[(0,u.jsxs)("span",{className:$.rollingWrap,children:[(0,u.jsx)("span",{style:{visibility:"hidden"},children:e}),(0,u.jsx)("span",{className:`${$.rollingNum} ${s==="up"?$.exitUp:$.exitDown}`,children:n},`o${n}-${e}`),(0,u.jsx)("span",{className:`${$.rollingNum} ${s==="up"?$.enterUp:$.enterDown}`,children:e},`n${e}`)]}),t?` ${t}`:""]})}function xy({activeType:e,onSelect:t,isDarkMode:n,sectionCount:o,onDetectSections:r,visible:i,onExited:s,placementCount:l,onClearPlacements:a,onDragStart:c,blankCanvas:d,onBlankCanvasChange:m,wireframePurpose:f,onWireframePurposeChange:k,Tooltip:w}){let{ref:C,mounted:S}=Vc(i,{onExited:s}),[g,x]=(0,Gt.useState)(!1),[h,E]=(0,Gt.useState)(!0),F=(0,Gt.useRef)(0),W=(0,Gt.useRef)(""),D=(0,Gt.useRef)(null),[j,Z]=(0,Gt.useState)(""),q=l>0||o>0,O=l+o;if(O>0&&(F.current=O,W.current=d?O===1?"Component":"Components":O===1?"Change":"Changes"),(0,Gt.useEffect)(()=>{if(q)g?E(!1):(E(!0),x(!0),Gi(()=>{Gi(()=>{E(!1)})}));else{E(!0);let ue=De(()=>x(!1),300);return()=>clearTimeout(ue)}},[q]),(0,Gt.useEffect)(()=>{if(!i)return;let ue=D.current;if(!ue)return;let J=()=>Z(Cw(ue));ue.addEventListener("scroll",J,{passive:!0});let Ce=new ResizeObserver(J);return Ce.observe(ue),()=>{ue.removeEventListener("scroll",J),Ce.disconnect()}},[i]),!S)return null;let Ae=[];return l>0&&Ae.push("placed"),o>0&&Ae.push("captured"),(0,u.jsxs)("div",{className:`${$.palette} ${n?"":$.light}`,ref:ue=>{C.current=ue,ue?.toggleAttribute("inert",!i)},"aria-hidden":!i,"data-feedback-toolbar":!0,"data-agentation-palette":!0,onClick:ue=>ue.stopPropagation(),onMouseDown:ue=>ue.stopPropagation(),children:[(0,u.jsxs)("div",{className:$.paletteHeader,children:[(0,u.jsx)("div",{className:$.paletteHeaderTitle,children:"Layout Mode"}),(0,u.jsxs)("div",{className:$.paletteHeaderDesc,children:["Rearrange and resize existing elements, add new components, and explore layout ideas. Agent results may vary."," ",(0,u.jsx)("a",{href:"https://agentation.com/features#layout-mode",target:"_blank",rel:"noopener noreferrer",children:"Learn more."})]})]}),(0,u.jsxs)("button",{type:"button","aria-pressed":d,className:`${$.canvasToggle} ${d?$.active:""}`,onClick:()=>m(!d),children:[(0,u.jsx)("span",{className:$.canvasToggleIcon,"aria-hidden":"true",children:(0,u.jsxs)("svg",{viewBox:"0 0 14 14",width:"14",height:"14",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"1",width:"12",height:"12",rx:"2",stroke:"currentColor",strokeWidth:"1"}),(0,u.jsx)("circle",{cx:"4.5",cy:"4.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"7",cy:"4.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"9.5",cy:"4.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"4.5",cy:"7",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"7",cy:"7",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"9.5",cy:"7",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"4.5",cy:"9.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"7",cy:"9.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"9.5",cy:"9.5",r:"0.8",fill:"currentColor",opacity:".6"})]})}),(0,u.jsx)("span",{className:$.canvasToggleLabel,children:"Wireframe New Page"})]}),(0,u.jsx)("div",{className:`${$.wireframePurposeWrap} ${d?"":$.collapsed}`,"aria-hidden":!d,ref:ue=>{ue?.toggleAttribute("inert",!d)},children:(0,u.jsx)("div",{className:$.wireframePurposeInner,children:(0,u.jsx)("textarea",{className:$.wireframePurposeInput,placeholder:"Describe this page to provide additional context for your agent.",value:f,onChange:ue=>k(ue.target.value),rows:2})})}),(0,u.jsx)(Mw,{activeType:e,onSelect:t,onDragStart:c,scrollRef:D,fadeClass:j,blankCanvas:d}),g&&(0,u.jsx)("div",{className:`${$.paletteFooterWrap} ${h?$.footerHidden:""}`,children:(0,u.jsx)("div",{className:$.paletteFooterInner,children:(0,u.jsx)("div",{className:$.paletteFooterInnerContent,children:(0,u.jsxs)("div",{className:$.paletteFooter,children:[(0,u.jsx)("span",{className:$.paletteFooterCount,children:(0,u.jsx)(Tw,{value:F.current,suffix:W.current})}),(0,u.jsx)("button",{className:$.paletteFooterClear,onClick:a,children:"Clear"})]})})})})]})}var be=ce(vt());var Lw=new Set(["nav","header","main","section","article","footer","aside"]),ym={banner:"Header",navigation:"Navigation",main:"Main Content",contentinfo:"Footer",complementary:"Sidebar",region:"Section"},vy={nav:"Navigation",header:"Header",main:"Main Content",section:"Section",article:"Article",footer:"Footer",aside:"Sidebar"},Rw=new Set(["script","style","noscript","link","meta"]),Pw=40;function wy(e){let t=e;for(;t&&t!==document.body&&t!==document.documentElement;){let n=window.getComputedStyle(t).position;if(n==="fixed"||n==="sticky")return!0;t=t.parentElement}return!1}function br(e){let t=e.tagName.toLowerCase();if(["nav","header","footer","main"].includes(t)&&document.querySelectorAll(t).length===1)return t;if(e.id)return`#${CSS.escape(e.id)}`;if(e.className&&typeof e.className=="string"){let r=e.className.split(/\s+/).filter(i=>i.length>0).find(i=>i.length>2&&!/^[a-zA-Z0-9]{6,}$/.test(i)&&!/^[a-z]{1,2}$/.test(i));if(r){let i=`${t}.${CSS.escape(r)}`;if(document.querySelectorAll(i).length===1)return i}}let n=e.parentElement;if(n){let r=Array.from(n.children).indexOf(e)+1;return`${n===document.body?"body":br(n)} > ${t}:nth-child(${r})`}return t}function Pl(e){let t=e.tagName.toLowerCase(),n=e.getAttribute("aria-label");if(n)return n;let o=e.getAttribute("role");if(o&&ym[o])return ym[o];if(vy[t])return vy[t];let r=e.querySelector("h1, h2, h3, h4, h5, h6");if(r){let s=r.textContent?.trim();if(s&&s.length<=50)return s;if(s)return s.slice(0,47)+"..."}let{name:i}=vr(e);return i.charAt(0).toUpperCase()+i.slice(1)}function by(e){let t=e.className;return typeof t!="string"||!t?null:t.split(/\s+/).map(o=>o.replace(/[_][a-zA-Z0-9]{5,}.*$/,"")).find(o=>o.length>2&&!/^[a-z]{1,2}$/.test(o))||null}function ky(e){let t=e.textContent?.trim();if(!t)return null;let n=t.replace(/\s+/g," ");return n.length<=30?n:n.slice(0,30)+"\u2026"}function Sy(){let e=document.querySelector("main")||document.body,t=Array.from(e.children),n=t;e!==document.body&&t.length<3&&(n=Array.from(document.body.children));let o=[];return n.forEach((r,i)=>{if(!(r instanceof HTMLElement))return;let s=r.tagName.toLowerCase();if(Rw.has(s)||r.hasAttribute("data-feedback-toolbar")||r.closest("[data-feedback-toolbar]"))return;let l=window.getComputedStyle(r);if(l.display==="none"||l.visibility==="hidden")return;let a=r.getBoundingClientRect();if(a.height<Pw)return;let c=Lw.has(s),d=r.getAttribute("role")&&ym[r.getAttribute("role")],m=s==="div"&&a.height>=60;if(!c&&!d&&!m)return;let f=window.scrollY,k=wy(r),w={x:a.x,y:k?a.y:a.y+f,width:a.width,height:a.height};o.push({id:`rs-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,label:Pl(r),tagName:s,selector:br(r),role:r.getAttribute("role"),className:by(r),textSnippet:ky(r),originalRect:w,currentRect:{...w},originalIndex:i,isFixed:k})}),o}function Cy(e){let t=window.scrollY,n=e.getBoundingClientRect(),o=wy(e),r={x:n.x,y:o?n.y:n.y+t,width:n.width,height:n.height},i=e.parentElement,s=0;return i&&(s=Array.from(i.children).indexOf(e)),{id:`rs-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,label:Pl(e),tagName:e.tagName.toLowerCase(),selector:br(e),role:e.getAttribute("role"),className:by(e),textSnippet:ky(e),originalRect:r,currentRect:{...r},originalIndex:s,isFixed:o}}var Qe=ce(ze()),Ey={bg:"rgba(59, 130, 246, 0.08)",border:"rgba(59, 130, 246, 0.5)",pill:"#3b82f6"},My=["nw","n","ne","e","se","s","sw","w"],Xc=24,Ty=16,Qc=5;function Ly(e,t,n,o){let r=1/0,i=1/0,s=e.x,l=e.x+e.width,a=e.x+e.width/2,c=e.y,d=e.y+e.height,m=e.y+e.height/2,f=[];for(let D of t)n.has(D.id)||f.push(D.currentRect);o&&f.push(...o);for(let D of f){let j=D.x,Z=D.x+D.width,q=D.x+D.width/2,O=D.y,Ae=D.y+D.height,ue=D.y+D.height/2;for(let J of[s,l,a])for(let Ce of[j,Z,q]){let K=Ce-J;Math.abs(K)<Qc&&Math.abs(K)<Math.abs(r)&&(r=K)}for(let J of[c,d,m])for(let Ce of[O,Ae,ue]){let K=Ce-J;Math.abs(K)<Qc&&Math.abs(K)<Math.abs(i)&&(i=K)}}let k=Math.abs(r)<Qc?r:0,w=Math.abs(i)<Qc?i:0,C=[],S=new Set,g=s+k,x=l+k,h=a+k,E=c+w,F=d+w,W=m+w;for(let D of f){let j=D.x,Z=D.x+D.width,q=D.x+D.width/2,O=D.y,Ae=D.y+D.height,ue=D.y+D.height/2;for(let J of[j,q,Z])for(let Ce of[g,h,x])if(Math.abs(Ce-J)<.5){let K=`x:${Math.round(J)}`;S.has(K)||(S.add(K),C.push({axis:"x",pos:J}))}for(let J of[O,ue,Ae])for(let Ce of[E,W,F])if(Math.abs(Ce-J)<.5){let K=`y:${Math.round(J)}`;S.has(K)||(S.add(K),C.push({axis:"y",pos:J}))}}return{dx:k,dy:w,guides:C}}var Nw=new Set(["script","style","noscript","link","meta","br","hr"]);function Ry(e){let t=e;for(;t&&t!==document.body&&t!==document.documentElement;){if(t.closest("[data-feedback-toolbar]"))return null;if(Nw.has(t.tagName.toLowerCase())){t=t.parentElement;continue}let n=t.getBoundingClientRect();if(n.width>=Ty&&n.height>=Ty)return t;t=t.parentElement}return null}function Py({rearrangeState:e,onChange:t,isDarkMode:n,exiting:o,className:r,blankCanvas:i,extraSnapRects:s,onSelectionChange:l,deselectSignal:a,onDragMove:c,onDragEnd:d,clearing:m}){let{sections:f}=e,k=(0,be.useRef)(e);k.current=e;let[w,C]=(0,be.useState)(new Set);(0,be.useEffect)(()=>{m&&C(new Set)},[m]);let S=(0,be.useRef)(a);(0,be.useEffect)(()=>{a!==S.current&&(S.current=a,C(new Set))},[a]);let[g,x]=(0,be.useState)(null),[h,E]=(0,be.useState)(!1),F=(0,be.useRef)(!1),W=(0,be.useCallback)(R=>{let M=f.find(B=>B.id===R);M&&(F.current=!!M.note,x(R),E(!1))},[f]),D=(0,be.useCallback)(()=>{g&&(E(!0),De(()=>{x(null),E(!1)},150))},[g]),j=(0,be.useCallback)(R=>{g&&(t({...e,sections:f.map(M=>M.id===g?{...M,note:R.trim()||void 0}:M)}),D())},[g,f,e,t,D]);(0,be.useEffect)(()=>{o&&g&&D()},[o]);let[Z,q]=(0,be.useState)(new Set),O=(0,be.useRef)(new Map),[Ae,ue]=(0,be.useState)(null),[J,Ce]=(0,be.useState)(null),[K,Ge]=(0,be.useState)([]),[Le,Ye]=(0,be.useState)(0),xe=(0,be.useRef)(null),gt=(0,be.useRef)(new Set),Ft=(0,be.useRef)(new Map),[Kn,_n]=(0,be.useState)(new Map),[Jn,Ve]=(0,be.useState)(new Map),vn=(0,be.useRef)(new Set),po=(0,be.useRef)(new Map),fo=(0,be.useRef)(l);fo.current=l;let Qo=(0,be.useRef)(c);Qo.current=c;let Go=(0,be.useRef)(d);Go.current=d,(0,be.useEffect)(()=>{i&&C(new Set)},[i]);let[On,Er]=(0,be.useState)(()=>!e.sections.some(R=>{let M=R.originalRect,B=R.currentRect;return Math.abs(M.x-B.x)>1||Math.abs(M.y-B.y)>1||Math.abs(M.width-B.width)>1||Math.abs(M.height-B.height)>1}));(0,be.useEffect)(()=>{if(!On){let R=De(()=>Er(!0),380);return()=>clearTimeout(R)}},[]);let Ot=(0,be.useRef)(new Set);(0,be.useEffect)(()=>{Ot.current=new Set(f.map(R=>R.selector))},[f]),(0,be.useEffect)(()=>{let R=()=>Ye(window.scrollY);return R(),window.addEventListener("scroll",R,{passive:!0}),window.addEventListener("resize",R,{passive:!0}),()=>{window.removeEventListener("scroll",R),window.removeEventListener("resize",R)}},[]),(0,be.useEffect)(()=>{let R=M=>{if(xe.current){ue(null);return}let B=document.elementFromPoint(M.clientX,M.clientY);if(!B){ue(null);return}if(B.closest("[data-feedback-toolbar]")){ue(null);return}if(B.closest("[data-design-placement]")){ue(null);return}if(B.closest("[data-annotation-popup]")){ue(null);return}let U=Ry(B);if(!U){ue(null);return}for(let ae of Ot.current)try{let Y=document.querySelector(ae);if(Y&&(Y===U||U.contains(Y))){ue(null);return}}catch{}let re=U.getBoundingClientRect();ue({x:re.x,y:re.y,w:re.width,h:re.height})};return document.addEventListener("mousemove",R,{passive:!0}),()=>document.removeEventListener("mousemove",R)},[f]),(0,be.useEffect)(()=>{let R=document.body.style.userSelect;return document.body.style.webkitUserSelect="none",document.body.style.userSelect="none",()=>{document.body.style.webkitUserSelect=R,document.body.style.userSelect=R}},[]),(0,be.useEffect)(()=>{let R=M=>{if(xe.current||M.button!==0)return;let B=M.composedPath()[0]??M.target;if(!B||B.closest("[data-feedback-toolbar]")||B.closest("[data-design-placement]")||B.closest("[data-annotation-popup]"))return;let U=Ry(B),re=!1;if(U)for(let Y of Ot.current)try{let ie=document.querySelector(Y);if(ie&&(ie===U||U.contains(ie))){re=!0;break}}catch{}let ae=!!(M.shiftKey||M.metaKey||M.ctrlKey);if(U&&!re){M.preventDefault(),M.stopPropagation();let Y=Cy(U),ie=[...f,Y],ve=[...e.originalOrder,Y.id];t({...e,sections:ie,originalOrder:ve});let Oe=new Set([Y.id]);C(Oe),fo.current?.(Oe,ae),ue(null);let it=M.clientX,Ze=M.clientY,_t={x:Y.currentRect.x,y:Y.currentRect.y},je=Y.originalRect,Se=!1,ut=0,Mt=0;xe.current="move";let st=et=>{let lt=et.clientX-it,yt=et.clientY-Ze;if(!Se&&(Math.abs(lt)>2||Math.abs(yt)>2)&&(Se=!0),!Se)return;let Yt={x:_t.x+lt,y:_t.y+yt,width:Y.currentRect.width,height:Y.currentRect.height},wn=Ly(Yt,ie,new Set([Y.id]),s);Ge(wn.guides);let me=lt+wn.dx,Kt=yt+wn.dy;ut=me,Mt=Kt;let Ln=Et().querySelector(`[data-rearrange-section="${Y.id}"]`);Ln&&(Ln.style.transform=`translate(${me}px, ${Kt}px)`),_n(new Map([[Y.id,{x:_t.x+me,y:_t.y+Kt,width:Y.currentRect.width,height:Y.currentRect.height}]])),Qo.current?.(me,Kt)},Ne=()=>{window.removeEventListener("mousemove",st),window.removeEventListener("mouseup",Ne),xe.current=null,Ge([]),_n(new Map);let et=Et().querySelector(`[data-rearrange-section="${Y.id}"]`);et&&(et.style.transform=""),Se&&t({...e,sections:ie.map(lt=>lt.id===Y.id?{...lt,currentRect:{...lt.currentRect,x:Math.max(0,_t.x+ut),y:Math.max(0,_t.y+Mt)}}:lt),originalOrder:ve}),Go.current?.(ut,Mt,Se)};window.addEventListener("mousemove",st),window.addEventListener("mouseup",Ne)}else if(re&&U){M.preventDefault();for(let Y of f)try{let ie=document.querySelector(Y.selector);if(ie&&ie===U){let ve=new Set([Y.id]);C(ve),fo.current?.(ve,ae);return}}catch{}ae||(C(new Set),fo.current?.(new Set,!1))}else ae||(C(new Set),fo.current?.(new Set,!1))};return document.addEventListener("mousedown",R,!0),()=>document.removeEventListener("mousedown",R,!0)},[f,e,t]),(0,be.useEffect)(()=>{let R=M=>{let B=M.composedPath()[0]||M.target;if(!(B.tagName==="INPUT"||B.tagName==="TEXTAREA"||B.isContentEditable)){if((M.key==="Backspace"||M.key==="Delete")&&w.size>0){M.preventDefault();let U=new Set(w);q(re=>{let ae=new Set(re);for(let Y of U)ae.add(Y);return ae}),C(new Set),De(()=>{let re=k.current;t({...re,sections:re.sections.filter(ae=>!U.has(ae.id)),originalOrder:re.originalOrder.filter(ae=>!U.has(ae))}),q(ae=>{let Y=new Set(ae);for(let ie of U)Y.delete(ie);return Y})},180);return}if(["ArrowUp","ArrowDown","ArrowLeft","ArrowRight"].includes(M.key)&&w.size>0){M.preventDefault();let U=M.shiftKey?20:1,re=M.key==="ArrowLeft"?-U:M.key==="ArrowRight"?U:0,ae=M.key==="ArrowUp"?-U:M.key==="ArrowDown"?U:0;t({...e,sections:f.map(Y=>w.has(Y.id)?{...Y,currentRect:{...Y.currentRect,x:Math.max(0,Y.currentRect.x+re),y:Math.max(0,Y.currentRect.y+ae)}}:Y)});return}M.key==="Escape"&&w.size>0&&C(new Set)}};return document.addEventListener("keydown",R),()=>document.removeEventListener("keydown",R)},[w,f,e,t]);let Zn=(0,be.useCallback)((R,M)=>{if(R.button!==0)return;let B=R.target;if(B.closest(`.${$.handle}`)||B.closest(`.${$.deleteButton}`))return;R.preventDefault(),R.stopPropagation();let U;R.shiftKey||R.metaKey||R.ctrlKey?(U=new Set(w),U.has(M)?U.delete(M):U.add(M)):w.has(M)?U=new Set(w):U=new Set([M]),C(U),(U.size!==w.size||[...U].some(Se=>!w.has(Se)))&&fo.current?.(U,!!(R.shiftKey||R.metaKey||R.ctrlKey));let ae=R.clientX,Y=R.clientY,ie=new Map;for(let Se of f)U.has(Se.id)&&ie.set(Se.id,{x:Se.currentRect.x,y:Se.currentRect.y});xe.current="move";let ve=!1,Oe=0,it=0,Ze=new Map;for(let Se of f)if(U.has(Se.id)){let ut=Et().querySelector(`[data-rearrange-section="${Se.id}"]`);Ze.set(Se.id,{outlineEl:ut,curW:Se.currentRect.width,curH:Se.currentRect.height})}let _t=Se=>{let ut=Se.clientX-ae,Mt=Se.clientY-Y;if(ut===0&&Mt===0)return;ve=!0;let st=1/0,Ne=1/0,et=-1/0,lt=-1/0;for(let[Kt,{curW:Ln,curH:Tr}]of Ze){let eo=ie.get(Kt);if(!eo)continue;let ho=eo.x+ut,ai=eo.y+Mt;st=Math.min(st,ho),Ne=Math.min(Ne,ai),et=Math.max(et,ho+Ln),lt=Math.max(lt,ai+Tr)}let yt=Ly({x:st,y:Ne,width:et-st,height:lt-Ne},f,U,s),Yt=ut+yt.dx,wn=Mt+yt.dy;Oe=Yt,it=wn,Ge(yt.guides);for(let[,{outlineEl:Kt}]of Ze)Kt&&(Kt.style.transform=`translate(${Yt}px, ${wn}px)`);let me=new Map;for(let[Kt,{curW:Ln,curH:Tr}]of Ze){let eo=ie.get(Kt);if(eo){let ho={x:Math.max(0,eo.x+Yt),y:Math.max(0,eo.y+wn),width:Ln,height:Tr};me.set(Kt,ho)}}_n(me),Qo.current?.(Yt,wn)},je=Se=>{window.removeEventListener("mousemove",_t),window.removeEventListener("mouseup",je),xe.current=null,Ge([]),_n(new Map);for(let[,{outlineEl:ut}]of Ze)ut&&(ut.style.transform="");if(ve){let ut=Se.clientX-ae,Mt=Se.clientY-Y;if(Math.abs(ut)<5&&Math.abs(Mt)<5)t({...e,sections:f.map(st=>{let Ne=ie.get(st.id);return Ne?{...st,currentRect:{...st.currentRect,x:Ne.x,y:Ne.y}}:st})});else{t({...e,sections:f.map(st=>{let Ne=ie.get(st.id);return Ne?{...st,currentRect:{...st.currentRect,x:Math.max(0,Ne.x+Oe),y:Math.max(0,Ne.y+it)}}:st})}),Go.current?.(Oe,it,!0);return}}Go.current?.(0,0,!1)};window.addEventListener("mousemove",_t),window.addEventListener("mouseup",je)},[w,f,e,t]),go=(0,be.useCallback)((R,M,B)=>{R.preventDefault(),R.stopPropagation();let U=f.find(je=>je.id===M);if(!U)return;C(new Set([M])),xe.current="resize";let re=R.clientX,ae=R.clientY,Y={...U.currentRect},ie=U.originalRect,ve=Y.width/Y.height,Oe={...Y},it=Et().querySelector(`[data-rearrange-section="${M}"]`),Ze=je=>{let Se=je.clientX-re,ut=je.clientY-ae,Mt=Y.x,st=Y.y,Ne=Y.width,et=Y.height;if(B.includes("e")&&(Ne=Math.max(Xc,Y.width+Se)),B.includes("w")&&(Ne=Math.max(Xc,Y.width-Se),Mt=Y.x+Y.width-Ne),B.includes("s")&&(et=Math.max(Xc,Y.height+ut)),B.includes("n")&&(et=Math.max(Xc,Y.height-ut),st=Y.y+Y.height-et),je.shiftKey)if(B.length===2){let yt=Math.abs(Ne-Y.width),Yt=Math.abs(et-Y.height);yt>Yt?et=Ne/ve:Ne=et*ve,B.includes("w")&&(Mt=Y.x+Y.width-Ne),B.includes("n")&&(st=Y.y+Y.height-et)}else B==="e"||B==="w"?et=Ne/ve:Ne=et*ve,B==="w"&&(Mt=Y.x+Y.width-Ne),B==="n"&&(st=Y.y+Y.height-et);Oe={x:Mt,y:st,width:Ne,height:et},it&&(it.style.left=`${Mt}px`,it.style.top=`${st-Le}px`,it.style.width=`${Ne}px`,it.style.height=`${et}px`),Ce({x:je.clientX+12,y:je.clientY+12,text:`${Math.round(Ne)} \xD7 ${Math.round(et)}`}),_n(new Map([[M,Oe]]))},_t=()=>{window.removeEventListener("mousemove",Ze),window.removeEventListener("mouseup",_t),Ce(null),xe.current=null,_n(new Map),t({...e,sections:f.map(je=>je.id===M?{...je,currentRect:Oe}:je)})};window.addEventListener("mousemove",Ze),window.addEventListener("mouseup",_t)},[f,e,t,Le]),Mr=(0,be.useCallback)(R=>{q(M=>{let B=new Set(M);return B.add(R),B}),C(M=>{let B=new Set(M);return B.delete(R),B}),De(()=>{let M=k.current;t({...M,sections:M.sections.filter(B=>B.id!==R),originalOrder:M.originalOrder.filter(B=>B!==R)}),q(B=>{let U=new Set(B);return U.delete(R),U})},180)},[t]),G=R=>{let M=R.originalRect,B=R.currentRect;return Math.abs(M.x-B.x)>1||Math.abs(M.y-B.y)>1||Math.abs(M.width-B.width)>1||Math.abs(M.height-B.height)>1},_e=R=>{let M=R.originalRect,B=R.currentRect;return Math.abs(M.x-B.x)>1||Math.abs(M.y-B.y)>1},Ee=R=>{let M=R.originalRect,B=R.currentRect;return Math.abs(M.width-B.width)>1||Math.abs(M.height-B.height)>1};for(let R of f)Ft.current.has(R.id)||(_e(R)?Ft.current.set(R.id,"move"):Ee(R)&&Ft.current.set(R.id,"resize"));for(let R of Ft.current.keys())f.some(M=>M.id===R)||Ft.current.delete(R);let Pe=f.filter(R=>{try{if(Z.has(R.id)||w.has(R.id))return!0;let M=document.querySelector(R.selector);if(!M)return!1;let B=M.getBoundingClientRect(),U=R.originalRect;return Math.abs(B.width-U.width)+Math.abs(B.height-U.height)<200}catch{return!1}}),ke=Pe.filter(R=>G(R)),dt=Pe.filter(R=>!G(R)),rt=new Set(ke.map(R=>R.id));for(let R of gt.current)rt.has(R)||gt.current.delete(R);let Fe=[...rt].sort().join(",");for(let R of ke)po.current.set(R.id,{currentRect:R.currentRect,originalRect:R.originalRect,isFixed:R.isFixed});(0,be.useEffect)(()=>{let R=vn.current;vn.current=rt;let M=new Map;for(let B of R)if(!rt.has(B)){if(!f.some(re=>re.id===B))continue;let U=po.current.get(B);U&&(M.set(B,{orig:U.originalRect,target:U.currentRect,isFixed:U.isFixed}),po.current.delete(B))}if(M.size>0){Ve(U=>{let re=new Map(U);for(let[ae,Y]of M)re.set(ae,Y);return re});let B=De(()=>{Ve(U=>{let re=new Map(U);for(let ae of M.keys())re.delete(ae);return re})},250);return()=>clearTimeout(B)}},[Fe,f]);let $e=(0,be.useRef)(null),Et=()=>$e.current?.getRootNode()??document;return(0,Qe.jsxs)(Qe.Fragment,{children:[(0,Qe.jsxs)("div",{ref:$e,className:`${$.rearrangeOverlay} ${n?"":$.light} ${o?$.overlayExiting:""}${r?` ${r}`:""}`,"data-feedback-toolbar":!0,children:[Ae&&(0,Qe.jsx)("div",{className:$.hoverHighlight,style:{left:Ae.x,top:Ae.y,width:Ae.w,height:Ae.h}}),dt.map(R=>{let M=R.currentRect,B=R.isFixed?M.y:M.y-Le,U=Ey,re=w.has(R.id);return(0,Qe.jsxs)("div",{"data-rearrange-section":R.id,className:`${$.sectionOutline} ${re?$.selected:""} ${m||o||Z.has(R.id)?$.exiting:""}`,style:{left:M.x,top:B,width:M.width,height:M.height,borderColor:U.border,backgroundColor:U.bg,...On?{}:{opacity:0,animation:"none",transition:"none"}},onMouseDown:ae=>Zn(ae,R.id),onDoubleClick:()=>W(R.id),children:[(0,Qe.jsx)("span",{className:$.sectionLabel,style:{backgroundColor:U.pill},children:R.label}),(0,Qe.jsx)("span",{className:`${$.sectionAnnotation} ${R.note?$.annotationVisible:""}`,children:(R.note&&O.current.set(R.id,R.note),R.note||O.current.get(R.id)||"")}),(0,Qe.jsxs)("span",{className:$.sectionDimensions,children:[Math.round(M.width)," \xD7 ",Math.round(M.height)]}),(0,Qe.jsx)("div",{className:$.deleteButton,onMouseDown:ae=>ae.stopPropagation(),onClick:()=>Mr(R.id),children:"\u2715"}),My.map(ae=>(0,Qe.jsx)("div",{className:`${$.handle} ${$[`handle${ae.charAt(0).toUpperCase()}${ae.slice(1)}`]}`,onMouseDown:Y=>go(Y,R.id,ae)},ae))]},R.id)}),ke.map(R=>{let M=R.currentRect,B=R.isFixed?M.y:M.y-Le,U=w.has(R.id),re=_e(R),ae=Ee(R);if(i&&!U)return null;let ie=!gt.current.has(R.id);return ie&&gt.current.add(R.id),(0,Qe.jsxs)("div",{"data-rearrange-section":R.id,className:`${$.ghostOutline} ${U?$.selected:""} ${m||o||Z.has(R.id)?$.exiting:""}`,style:{left:M.x,top:B,width:M.width,height:M.height,...On?{}:{opacity:0,animation:"none",transition:"none"},...ie?{}:{animation:"none"}},onMouseDown:ve=>Zn(ve,R.id),onDoubleClick:()=>W(R.id),children:[(0,Qe.jsx)("span",{className:$.sectionLabel,style:{backgroundColor:Ey.pill},children:R.label}),(0,Qe.jsx)("span",{className:`${$.sectionAnnotation} ${R.note?$.annotationVisible:""}`,children:(R.note&&O.current.set(R.id,R.note),R.note||O.current.get(R.id)||"")}),(0,Qe.jsxs)("span",{className:$.sectionDimensions,children:[Math.round(M.width)," \xD7 ",Math.round(M.height)]}),(0,Qe.jsx)("div",{className:$.deleteButton,onMouseDown:ve=>ve.stopPropagation(),onClick:()=>Mr(R.id),children:"\u2715"}),My.map(ve=>(0,Qe.jsx)("div",{className:`${$.handle} ${$[`handle${ve.charAt(0).toUpperCase()}${ve.slice(1)}`]}`,onMouseDown:Oe=>go(Oe,R.id,ve)},ve)),(0,Qe.jsx)("span",{className:$.ghostBadge,children:(()=>{let ve=Ft.current.get(R.id);if(re&&ae){let[Oe,it]=ve==="resize"?["Resize","Move"]:["Move","Resize"];return(0,Qe.jsxs)(Qe.Fragment,{children:["Suggested ",Oe," ",(0,Qe.jsxs)("span",{className:$.ghostBadgeExtra,children:["& ",it]})]})}return`Suggested ${ae?"Resize":"Move"}`})()})]},R.id)})]}),!i&&(()=>{let R=[];for(let M of ke){let B=Kn.get(M.id);R.push({id:M.id,orig:M.originalRect,target:B||M.currentRect,isFixed:M.isFixed,isSelected:w.has(M.id),isExiting:Z.has(M.id)})}for(let[M,B]of Kn)if(!R.some(U=>U.id===M)){let U=f.find(re=>re.id===M);U&&R.push({id:M,orig:U.originalRect,target:B,isFixed:U.isFixed,isSelected:w.has(M)})}for(let[M,B]of Jn)R.some(U=>U.id===M)||R.push({id:M,orig:B.orig,target:B.target,isFixed:B.isFixed,isSelected:!1,isExiting:!0});return R.length===0?null:(0,Qe.jsxs)("svg",{className:`${$.connectorSvg} ${m||o?$.connectorExiting:""}`,children:[R.map(({id:M,orig:B,target:U,isFixed:re,isSelected:ae,isExiting:Y})=>{let ie=B.x+B.width/2,ve=(re?B.y:B.y-Le)+B.height/2,Oe=U.x+U.width/2,it=(re?U.y:U.y-Le)+U.height/2,Ze=Oe-ie,_t=it-ve,je=Math.sqrt(Ze*Ze+_t*_t);if(je<2)return null;let Se=Math.min(1,je/40),ut=Math.min(je*.3,60),Mt=je>0?-_t/je:0,st=je>0?Ze/je:0,Ne=(ie+Oe)/2+Mt*ut,et=(ve+it)/2+st*ut,lt=Kn.has(M),yt=lt||ae?1:.4,Yt=lt||ae?1:.5;return(0,Qe.jsxs)("g",{className:Y?$.connectorExiting:"",children:[(0,Qe.jsx)("path",{className:$.connectorLine,d:`M ${ie} ${ve} Q ${Ne} ${et} ${Oe} ${it}`,fill:"none",stroke:"rgba(59, 130, 246, 0.45)",strokeWidth:"1.5",opacity:yt*Se}),(0,Qe.jsx)("circle",{className:$.connectorDot,cx:ie,cy:ve,r:4*Se,fill:"rgba(59, 130, 246, 0.8)",stroke:"#fff",strokeWidth:"1.5",opacity:Yt*Se,filter:"url(#connDotShadow)"}),(0,Qe.jsx)("circle",{className:$.connectorDot,cx:Oe,cy:it,r:4*Se,fill:"rgba(59, 130, 246, 0.8)",stroke:"#fff",strokeWidth:"1.5",opacity:Yt*Se,filter:"url(#connDotShadow)"})]},`conn-${M}`)}),(0,Qe.jsx)("defs",{children:(0,Qe.jsx)("filter",{id:"connDotShadow",x:"-50%",y:"-50%",width:"200%",height:"200%",children:(0,Qe.jsx)("feDropShadow",{dx:"0",dy:"0.5",stdDeviation:"1",floodOpacity:"0.15"})})})]})})(),g&&(()=>{let R=f.find(it=>it.id===g);if(!R)return null;let M=R.currentRect,B=R.isFixed?M.y:M.y-Le,U=M.x+M.width/2,re=B-8,ae=B+M.height+8,Y=re>200,ie=ae<window.innerHeight-100,ve=Math.max(160,Math.min(window.innerWidth-160,U)),Oe;return Y?Oe={left:ve,bottom:window.innerHeight-re}:ie?Oe={left:ve,top:ae}:Oe={left:ve,top:Math.max(80,window.innerHeight/2-80)},(0,Qe.jsx)(is,{element:R.label,placeholder:"Add a note about this section",initialValue:R.note??"",submitLabel:F.current?"Save":"Set",onSubmit:j,onCancel:D,onDelete:F.current?()=>{j("")}:void 0,isExiting:h,lightMode:!n,style:Oe})})(),J&&(0,Qe.jsx)("div",{className:$.sizeIndicator,style:{left:J.x,top:J.y},"data-feedback-toolbar":!0,children:J.text}),K.map((R,M)=>(0,Qe.jsx)("div",{className:$.guideLine,style:R.axis==="x"?{position:"fixed",left:R.pos,top:0,width:1,height:"100vh"}:{position:"fixed",left:0,top:R.pos-Le,width:"100vw",height:1}},`${R.axis}-${R.pos}-${M}`))]})}var xm=new Set(["script","style","noscript","link","meta","br","hr"]);function Iw(){let e=document.querySelector("main")||document.body,t=[],n=Array.from(e.children),o=e!==document.body&&n.length<3?Array.from(document.body.children):n;for(let r of o){if(!(r instanceof HTMLElement)||xm.has(r.tagName.toLowerCase())||r.hasAttribute("data-feedback-toolbar"))continue;let i=window.getComputedStyle(r);if(i.display==="none"||i.visibility==="hidden")continue;let s=r.getBoundingClientRect();if(!(s.height<10||s.width<10)){t.push({label:Pl(r),selector:br(r),top:s.top,bottom:s.bottom,left:s.left,right:s.right,area:s.width*s.height});for(let l of Array.from(r.children)){if(!(l instanceof HTMLElement)||xm.has(l.tagName.toLowerCase())||l.hasAttribute("data-feedback-toolbar"))continue;let a=window.getComputedStyle(l);if(a.display==="none"||a.visibility==="hidden")continue;let c=l.getBoundingClientRect();c.height<10||c.width<10||t.push({label:Pl(l),selector:br(l),top:c.top,bottom:c.bottom,left:c.left,right:c.right,area:c.width*c.height})}}}return t}function Dw(e){let t=window.scrollY;return e.map(({label:n,selector:o,rect:r})=>{let i=r.y-t;return{label:n,selector:o,top:i,bottom:i+r.height,left:r.x,right:r.x+r.width,area:r.width*r.height}})}function $w(e){let t=window.scrollY,n=e.y-t,o=e.x;return{top:n,bottom:n+e.height,left:o,right:o+e.width,area:e.width*e.height}}function Gc(e,t){let n=t?Dw(t):Iw(),o=$w(e),r=null,i=null,s=null,l=null,a=null;for(let w of n){if(Math.abs(w.left-o.left)<2&&Math.abs(w.top-o.top)<2&&Math.abs(w.right-w.left-e.width)<2&&Math.abs(w.bottom-w.top-e.height)<2)continue;w.left<=o.left+2&&w.right>=o.right-2&&w.top<=o.top+2&&w.bottom>=o.bottom-2&&w.area>o.area*1.5&&(!a||w.area<a._area)&&(a={label:w.label,selector:w.selector,_area:w.area});let C=o.right>w.left+5&&o.left<w.right-5,S=o.bottom>w.top+5&&o.top<w.bottom-5;if(C&&w.bottom<=o.top+5){let g=Math.round(o.top-w.bottom);(!r||g<r._dist)&&(r={label:w.label,selector:w.selector,gap:Math.max(0,g),_dist:g})}if(C&&w.top>=o.bottom-5){let g=Math.round(w.top-o.bottom);(!i||g<i._dist)&&(i={label:w.label,selector:w.selector,gap:Math.max(0,g),_dist:g})}if(S&&w.right<=o.left+5){let g=Math.round(o.left-w.right);(!s||g<s._dist)&&(s={label:w.label,selector:w.selector,gap:Math.max(0,g),_dist:g})}if(S&&w.left>=o.right-5){let g=Math.round(w.left-o.right);(!l||g<l._dist)&&(l={label:w.label,selector:w.selector,gap:Math.max(0,g),_dist:g})}}let c=window.innerWidth,d=window.innerHeight,m=Fw(e,c),f=w=>w?{label:w.label,selector:w.selector,gap:w.gap}:null,k=Aw(o,e,c,d,a?{label:a.label,selector:a.selector,_area:a._area}:null,n);return{above:f(r),below:f(i),left:f(s),right:f(l),alignment:m,containedIn:a?{label:a.label,selector:a.selector}:null,outOfBounds:k}}function Aw(e,t,n,o,r,i){let s={},l=!1,a=[];if(e.left<-2&&a.push("left"),e.right>n+2&&a.push("right"),e.top<-2&&a.push("top"),e.bottom>o+2&&a.push("bottom"),a.length>0&&(s.viewport=a,l=!0),r){let c=i.find(d=>d.label===r.label&&d.selector===r.selector&&Math.abs(d.area-r._area)<10);if(c){let d=[];e.left<c.left-2&&d.push("left"),e.right>c.right+2&&d.push("right"),e.top<c.top-2&&d.push("top"),e.bottom>c.bottom+2&&d.push("bottom"),d.length>0&&(s.container={label:r.label,edges:d},l=!0)}}return l?s:null}function Fw(e,t){if(e.width/t>.85)return"full-width";let o=e.x+e.width/2,r=t/2,i=o-r,s=t*.08;return Math.abs(i)<s?"center":i<0?"left":"right"}function Dy(e){switch(e){case"full-width":return"full-width";case"center":return"centered";case"left":return"left-aligned";case"right":return"right-aligned"}}function vm(e,t={}){let n=[];e.above&&n.push(`Below \`${e.above.label}\`${e.above.gap>0?` (${e.above.gap}px gap)`:""}`),e.below&&n.push(`Above \`${e.below.label}\`${e.below.gap>0?` (${e.below.gap}px gap)`:""}`),t.includeLeftRight&&(e.left&&n.push(`Right of \`${e.left.label}\`${e.left.gap>0?` (${e.left.gap}px gap)`:""}`),e.right&&n.push(`Left of \`${e.right.label}\`${e.right.gap>0?` (${e.right.gap}px gap)`:""}`));let o=Dy(e.alignment);return e.containedIn?n.push(`${o.charAt(0).toUpperCase()+o.slice(1)} in \`${e.containedIn.label}\``):n.push(`${o.charAt(0).toUpperCase()+o.slice(1)} in page`),t.includePixelRef&&t.pixelRef&&n.push(`Pixel ref: \`${t.pixelRef}\``),e.outOfBounds&&(e.outOfBounds.viewport&&n.push(`**Outside viewport** (${e.outOfBounds.viewport.join(", ")} edge${e.outOfBounds.viewport.length>1?"s":""})`),e.outOfBounds.container&&n.push(`**Outside \`${e.outOfBounds.container.label}\`** (${e.outOfBounds.container.edges.join(", ")} edge${e.outOfBounds.container.edges.length>1?"s":""})`)),n}function $y(e,t,n){let o=[];e.above&&o.push(`below \`${e.above.label}\``),e.below&&o.push(`above \`${e.below.label}\``),e.left&&o.push(`right of \`${e.left.label}\``),e.right&&o.push(`left of \`${e.right.label}\``),e.containedIn&&o.push(`inside \`${e.containedIn.label}\``),o.push(Dy(e.alignment)),e.outOfBounds?.viewport&&o.push(`**outside viewport** (${e.outOfBounds.viewport.join(", ")})`),e.outOfBounds?.container&&o.push(`**outside \`${e.outOfBounds.container.label}\`** (${e.outOfBounds.container.edges.join(", ")})`);let r=n?`, ${Math.round(n.width)}\xD7${Math.round(n.height)}px`:"";return`at (${Math.round(t.x)}, ${Math.round(t.y)})${r}: ${o.join(", ")}`}var Ny=15;function Iy(e){if(e.length<2)return[];let t=[],n=new Set;for(let o=0;o<e.length;o++){if(n.has(o))continue;let r=[o];for(let i=o+1;i<e.length;i++)n.has(i)||Math.abs(e[o].rect.y-e[i].rect.y)<Ny&&r.push(i);if(r.length>=2){let i=r.map(a=>e[a]);i.sort((a,c)=>a.rect.x-c.rect.x);let s=[];for(let a=0;a<i.length-1;a++)s.push(Math.round(i[a+1].rect.x-(i[a].rect.x+i[a].rect.width)));let l=Math.round(i.reduce((a,c)=>a+c.rect.y,0)/i.length);t.push({labels:i.map(a=>a.label),type:"row",sharedEdge:l,gaps:s,avgGap:s.length?Math.round(s.reduce((a,c)=>a+c,0)/s.length):0}),r.forEach(a=>n.add(a))}}for(let o=0;o<e.length;o++){if(n.has(o))continue;let r=[o];for(let i=o+1;i<e.length;i++)n.has(i)||Math.abs(e[o].rect.x-e[i].rect.x)<Ny&&r.push(i);if(r.length>=2){let i=r.map(a=>e[a]);i.sort((a,c)=>a.rect.y-c.rect.y);let s=[];for(let a=0;a<i.length-1;a++)s.push(Math.round(i[a+1].rect.y-(i[a].rect.y+i[a].rect.height)));let l=Math.round(i.reduce((a,c)=>a+c.rect.x,0)/i.length);t.push({labels:i.map(a=>a.label),type:"column",sharedEdge:l,gaps:s,avgGap:s.length?Math.round(s.reduce((a,c)=>a+c,0)/s.length):0}),r.forEach(a=>n.add(a))}}return t}function Ay(e){if(e.length<2)return[];let t=Iy(e.map(s=>({label:s.label,rect:s.originalRect}))),n=Iy(e.map(s=>({label:s.label,rect:s.currentRect}))),o=[],r=new Set;for(let s of t){let l=new Set(s.labels),a=null,c=0;for(let d of n){let m=d.labels.filter(f=>l.has(f)).length;m>=2&&m>c&&(a=d,c=m)}if(a){let d=a.labels.filter(f=>l.has(f)),m=d.join(", ");if(a.type!==s.type){let f=s.type==="row"?"y":"x",k=a.type==="row"?"y":"x";o.push(`**${m}**: ${s.type} (${f}\u2248${s.sharedEdge}, ${s.avgGap}px gaps) \u2192 ${a.type} (${k}\u2248${a.sharedEdge}, ${a.avgGap}px gaps)`)}else if(Math.abs(s.sharedEdge-a.sharedEdge)>20||Math.abs(s.avgGap-a.avgGap)>5){let f=s.type==="row"?"y":"x",k=Math.abs(s.sharedEdge-a.sharedEdge)>20?` ${f}: ${s.sharedEdge} \u2192 ${a.sharedEdge}`:"",w=Math.abs(s.avgGap-a.avgGap)>5?` gaps: ${s.avgGap}px \u2192 ${a.avgGap}px`:"";o.push(`**${m}**: ${s.type} shifted \u2014${k}${w}`)}d.forEach(f=>r.add(f))}else{let d=s.labels.join(", "),m=s.type==="row"?"y":"x";o.push(`**${d}**: ${s.type} (${m}\u2248${s.sharedEdge}) dissolved`),s.labels.forEach(f=>r.add(f))}}for(let s of n){if(s.labels.every(c=>r.has(c))||s.labels.filter(c=>!r.has(c)).length<2)continue;if(!t.some(c=>c.labels.filter(m=>s.labels.includes(m)).length>=2)){let c=s.type==="row"?"y":"x";o.push(`**${s.labels.join(", ")}**: new ${s.type} (${c}\u2248${s.sharedEdge}, ${s.avgGap}px gaps)`),s.labels.forEach(d=>r.add(d))}}let i=e.filter(s=>!r.has(s.label));if(i.length>=2){let s={};for(let l of i){let a=Math.round(l.currentRect.x/5)*5;(s[a]??(s[a]=[])).push(l.label)}for(let[l,a]of Object.entries(s))a.length>=2&&o.push(`**${a.join(", ")}**: shared left edge at x\u2248${l}`)}return o}function wm(e){if(typeof document>"u")return{viewport:e,contentArea:null};let t=[],n=new Set,o=l=>{n.has(l)||l instanceof HTMLElement&&(l.hasAttribute("data-feedback-toolbar")||xm.has(l.tagName.toLowerCase())||(n.add(l),t.push(l)))},r=document.querySelector("main");r&&o(r);let i=document.querySelector("[role='main']");i&&o(i);for(let l of Array.from(document.body.children))if(o(l),l.children){for(let a of Array.from(l.children))if(o(a),a.children)for(let c of Array.from(a.children))o(c)}let s=null;for(let l of t){let a=l.getBoundingClientRect();if(a.height<50)continue;let c=getComputedStyle(l);if(c.maxWidth&&c.maxWidth!=="none"&&c.maxWidth!=="0px"){(!s||a.width<s.rect.width)&&(s={el:l,rect:a});continue}!s&&a.width<e.width-20&&a.width>100&&(s={el:l,rect:a})}if(s){let{el:l,rect:a}=s;return{viewport:e,contentArea:{width:Math.round(a.width),left:Math.round(a.left),right:Math.round(a.right),centerX:Math.round(a.left+a.width/2),selector:br(l)}}}return{viewport:e,contentArea:null}}function Fy(e){if(typeof document>"u")return null;let t=document.querySelector(e);if(!t?.parentElement)return null;let n=getComputedStyle(t.parentElement),o={parentDisplay:n.display,parentSelector:br(t.parentElement)};return n.display.includes("flex")&&(o.flexDirection=n.flexDirection),n.display.includes("grid")&&n.gridTemplateColumns!=="none"&&(o.gridCols=n.gridTemplateColumns),n.gap&&n.gap!=="normal"&&n.gap!=="0px"&&(o.gap=n.gap),o}function bm(e,t){let n=t.contentArea,o=n?n.width:t.viewport.width,r=n?n.left:0,i=n?n.centerX:Math.round(t.viewport.width/2),s=Math.round(e.x-r),l=Math.round(r+o-(e.x+e.width)),a=(e.width/o*100).toFixed(1),c=e.x+e.width/2,d=Math.abs(c-i)<20,m=e.width/o>.95,f=[];return m?f.push("`width: 100%` of container"):f.push(`left \`${s}px\` in container, right \`${l}px\`, width \`${a}%\` (\`${Math.round(e.width)}px\`)`),d&&!m&&f.push("centered \u2014 `margin-inline: auto`"),f.join(" \u2014 ")}function Oy(e){let{viewport:t,contentArea:n}=e,o=`### Reference Frame
`;if(o+=`- Viewport: \`${t.width}\xD7${t.height}px\`
`,n){let r=n;o+=`- Content area: \`${r.width}px\` wide, left edge at \`x=${r.left}\`, right at \`x=${r.right}\` (\`${r.selector}\`)
`,o+=`- Pixel \u2192 CSS translation:
`,o+=`  - **Horizontal position in container**: \`element.x - ${r.left}\` \u2192 use as \`margin-left\` or \`left\`
`,o+=`  - **Width as % of container**: \`element.width / ${r.width} \xD7 100\` \u2192 use as \`width: X%\`
`,o+="  - **Vertical gap between elements**: `nextElement.y - (prevElement.y + prevElement.height)` \u2192 use as `margin-top` or `gap`\n",o+=`  - **Centered**: if \`|element.centerX - ${r.centerX}| < 20px\` \u2192 use \`margin-inline: auto\`
`}else o+=`- No distinct content container \u2014 elements positioned relative to full viewport
`,o+=`- Pixel \u2192 CSS translation:
`,o+=`  - **Width as % of viewport**: \`element.width / ${t.width} \xD7 100\` \u2192 use as \`width: X%\`
`,o+=`  - **Centered**: if \`|(element.x + element.width/2) - ${Math.round(t.width/2)}| < 20px\` \u2192 use \`margin-inline: auto\`
`;return o+=`
`,o}function Ow(e){let t=Fy(e);if(!t)return null;let n=`\`${t.parentDisplay}\``;return t.flexDirection&&(n+=`, flex-direction: \`${t.flexDirection}\``),t.gridCols&&(n+=`, grid-template-columns: \`${t.gridCols}\``),t.gap&&(n+=`, gap: \`${t.gap}\``),`Parent: ${n} (\`${t.parentSelector}\`)`}function km(e,t,n,o="standard"){if(e.length===0)return"";let r=[...e].sort((S,g)=>Math.abs(S.y-g.y)<20?S.x-g.x:S.y-g.y),i="";if(n?.blankCanvas?(i+=`## Wireframe: New Page

`,n.wireframePurpose&&(i+=`> **Purpose:** ${n.wireframePurpose}
>
`),i+=`> ${e.length} component${e.length!==1?"s":""} placed \u2014 this is a standalone wireframe, not related to the current page.
>
> This wireframe is a rough sketch for exploring ideas.

`):i+=`## Design Layout

> ${e.length} component${e.length!==1?"s":""} placed

`,o==="compact")return i+=`### Components
`,r.forEach((S,g)=>{let x=Fn[S.type]?.label||S.type;i+=`${g+1}. **${x}** \u2014 \`${Math.round(S.width)}\xD7${Math.round(S.height)}px\` at \`(${Math.round(S.x)}, ${Math.round(S.y)})\`
`,S.text&&(i+=`   - Note: "${S.text}"
`)}),i;let s=wm(t);i+=Oy(s),i+=`### Components
`,r.forEach((S,g)=>{let x=Fn[S.type]?.label||S.type,h={x:S.x,y:S.y,width:S.width,height:S.height};i+=`${g+1}. **${x}** \u2014 \`${Math.round(S.width)}\xD7${Math.round(S.height)}px\` at \`(${Math.round(S.x)}, ${Math.round(S.y)})\`
`,S.text&&(i+=`   - Note: "${S.text}"
`);let E=Gc(h),W=vm(E,{includeLeftRight:o==="detailed"||o==="forensic"});for(let j of W)i+=`   - ${j}
`;let D=bm(h,s);D&&(i+=`   - CSS: ${D}
`)}),i+=`
### Layout Analysis
`;let l=[];for(let S of r){let g=l.find(x=>Math.abs(x.y-S.y)<30);g?g.items.push(S):l.push({y:S.y,items:[S]})}if(l.sort((S,g)=>S.y-g.y),l.forEach((S,g)=>{S.items.sort((h,E)=>h.x-E.x);let x=S.items.map(h=>Fn[h.type]?.label||h.type);if(S.items.length===1){let E=S.items[0].width>t.width*.8;i+=`- Row ${g+1} (y\u2248${Math.round(S.y)}): ${x[0]}${E?" \u2014 full width":""}
`}else i+=`- Row ${g+1} (y\u2248${Math.round(S.y)}): ${x.join(" | ")} \u2014 ${S.items.length} items side by side
`}),o==="detailed"||o==="forensic"){i+=`
### Spacing & Gaps
`;for(let S=0;S<r.length-1;S++){let g=r[S],x=r[S+1],h=Fn[g.type]?.label||g.type,E=Fn[x.type]?.label||x.type,F=Math.round(x.y-(g.y+g.height)),W=Math.round(x.x-(g.x+g.width));Math.abs(g.y-x.y)<30?i+=`- ${h} \u2192 ${E}: \`${W}px\` horizontal gap
`:i+=`- ${h} \u2192 ${E}: \`${F}px\` vertical gap
`}if(o==="forensic"&&r.length>2){i+=`
### All Pairwise Gaps
`;for(let S=0;S<r.length;S++)for(let g=S+1;g<r.length;g++){let x=r[S],h=r[g],E=Fn[x.type]?.label||x.type,F=Fn[h.type]?.label||h.type,W=Math.round(h.y-(x.y+x.height)),D=Math.round(h.x-(x.x+x.width));i+=`- ${E} \u2194 ${F}: h=\`${D}px\` v=\`${W}px\`
`}}o==="forensic"&&(i+=`
### Z-Order (placement order)
`,e.forEach((S,g)=>{let x=Fn[S.type]?.label||S.type;i+=`${g}. ${x} at \`(${Math.round(S.x)}, ${Math.round(S.y)})\`
`}))}i+=`
### Suggested Implementation
`;let a=r.some(S=>S.type==="navigation"),c=r.some(S=>S.type==="hero"),d=r.some(S=>S.type==="sidebar"),m=r.some(S=>S.type==="footer"),f=r.filter(S=>S.type==="card"),k=r.filter(S=>S.type==="form"),w=r.filter(S=>S.type==="table"),C=r.filter(S=>S.type==="modal");if(a&&(i+=`- Top navigation bar with logo + nav links + CTA
`),c&&(i+=`- Hero section with heading, subtext, and call-to-action
`),d&&(i+=`- Sidebar layout \u2014 use CSS Grid with sidebar + main content area
`),f.length>1?i+=`- ${f.length}-column card grid \u2014 use CSS Grid or Flexbox
`:f.length===1&&(i+=`- Card component with image + content area
`),k.length>0&&(i+=`- ${k.length} form${k.length>1?"s":""} \u2014 add proper labels, validation, and submit handling
`),w.length>0&&(i+=`- Data table \u2014 consider sortable columns and pagination
`),C.length>0&&(i+=`- Modal dialog \u2014 add overlay backdrop and focus trapping
`),m&&(i+=`- Multi-column footer with links
`),o==="detailed"||o==="forensic"){if(i+=`
### CSS Suggestions
`,d){let S=r.find(g=>g.type==="sidebar");i+=`- \`display: grid; grid-template-columns: ${Math.round(S.width)}px 1fr;\`
`}if(f.length>1){let S=Math.round(f[0].width);i+=`- \`display: grid; grid-template-columns: repeat(${f.length}, ${S}px); gap: 16px;\`
`}a&&(i+="- Navigation: `position: sticky; top: 0; z-index: 50;`\n")}return i}function Sm(e,t="standard",n){let{sections:o}=e,r=[];for(let d of o){let m=d.originalRect,f=d.currentRect,k=Math.abs(m.x-f.x)>1||Math.abs(m.y-f.y)>1,w=Math.abs(m.width-f.width)>1||Math.abs(m.height-f.height)>1,C=!!d.note;if(!k&&!w&&!C){t==="forensic"&&r.push({section:d,posMoved:!1,sizeChanged:!1});continue}r.push({section:d,posMoved:k,sizeChanged:w})}if(r.length===0||t!=="forensic"&&r.every(d=>!d.posMoved&&!d.sizeChanged&&!d.section.note))return"";let i=`## Suggested Layout Changes

`,s=n?n.width:typeof window<"u"?window.innerWidth:0,l=n?n.height:typeof window<"u"?window.innerHeight:0,a=wm({width:s,height:l});t!=="compact"&&(i+=Oy(a)),t==="forensic"&&(i+=`> Detected at: \`${new Date(e.detectedAt).toISOString()}\`
`,i+=`> Total sections: ${o.length}

`);let c=d=>o.map(m=>({label:m.label,selector:m.selector,rect:d==="original"?m.originalRect:m.currentRect}));i+=`**Changes:**
`;for(let{section:d,posMoved:m,sizeChanged:f}of r){let k=d.originalRect,w=d.currentRect;if(!m&&!f){d.note?(i+=`- **${d.label}** \u2014 note only
`,i+=`  - Note: "${d.note}"
`):i+=`- ${d.label} \u2014 unchanged at (${Math.round(w.x)}, ${Math.round(w.y)}) ${Math.round(w.width)}\xD7${Math.round(w.height)}px
`;continue}if(t==="compact"){m&&f?i+=`- Suggested: move **${d.label}** to (${Math.round(w.x)}, ${Math.round(w.y)}) ${Math.round(w.width)}\xD7${Math.round(w.height)}px
`:m?i+=`- Suggested: move **${d.label}** to (${Math.round(w.x)}, ${Math.round(w.y)})
`:i+=`- Suggested: resize **${d.label}** to ${Math.round(w.width)}\xD7${Math.round(w.height)}px
`,d.note&&(i+=`  - Note: "${d.note}"
`);continue}if(m&&f?i+=`- Suggested: move and resize **${d.label}**
`:m?i+=`- Suggested: move **${d.label}**
`:i+=`- Suggested: resize **${d.label}** from ${Math.round(k.width)}\xD7${Math.round(k.height)}px to ${Math.round(w.width)}\xD7${Math.round(w.height)}px
`,d.note&&(i+=`  - Note: "${d.note}"
`),m){let S=Gc(k,c("original")),g=Gc(w,c("current")),x=f?{width:k.width,height:k.height}:void 0;i+=`  - Currently ${$y(S,{x:k.x,y:k.y},x)}
`;let h=f?{width:w.width,height:w.height}:void 0,E=`at (${Math.round(w.x)}, ${Math.round(w.y)})`,F=h?`, ${Math.round(h.width)}\xD7${Math.round(h.height)}px`:"",D=vm(g,{includeLeftRight:t==="detailed"||t==="forensic"});if(D.length>0){i+=`  - Suggested position ${E}${F}: ${D[0]}
`;for(let Z=1;Z<D.length;Z++)i+=`    ${D[Z]}
`}else i+=`  - Suggested position ${E}${F}
`;let j=bm(w,a);j&&(i+=`  - CSS: ${j}
`)}let C=Ow(d.selector);if(C&&(i+=`  - ${C}
`),i+=`  - Selector: \`${d.selector}\`
`,t==="detailed"||t==="forensic"){let S=d.className?`${d.tagName}.${d.className.split(" ")[0]}`:d.tagName;S!==d.selector&&(i+=`  - Element: \`${S}\`
`),d.role&&(i+=`  - Role: \`${d.role}\`
`),t==="forensic"&&d.textSnippet&&(i+=`  - Text: "${d.textSnippet}"
`)}t==="forensic"&&(i+=`  - Original rect: \`{ x: ${Math.round(k.x)}, y: ${Math.round(k.y)}, w: ${Math.round(k.width)}, h: ${Math.round(k.height)} }\`
`,i+=`  - Current rect: \`{ x: ${Math.round(w.x)}, y: ${Math.round(w.y)}, w: ${Math.round(w.width)}, h: ${Math.round(w.height)} }\`
`)}if(t!=="compact"){let d=r.filter(f=>f.posMoved).map(f=>({label:f.section.label,originalRect:f.section.originalRect,currentRect:f.section.currentRect})),m=Ay(d);if(m.length>0){i+=`
### Layout Summary
`;for(let f of m)i+=`- ${f}
`}}if(t!=="compact"&&o.length>1){i+=`
### All Sections (current positions)
`;let d=[...o].sort((m,f)=>Math.abs(m.currentRect.y-f.currentRect.y)<20?m.currentRect.x-f.currentRect.x:m.currentRect.y-f.currentRect.y);for(let m of d){let f=m.currentRect,k=Math.abs(f.x-m.originalRect.x)>1||Math.abs(f.y-m.originalRect.y)>1||Math.abs(f.width-m.originalRect.width)>1||Math.abs(f.height-m.originalRect.height)>1;i+=`- ${m.label}: \`${Math.round(f.width)}\xD7${Math.round(f.height)}px\` at \`(${Math.round(f.x)}, ${Math.round(f.y)})\`${k?" \u2190 suggested":""}
`}}return i}var Cm="feedback-annotations-";function Nl(e){return`${Cm}${e}`}function Xo(e){if(typeof window>"u")return[];try{let t=localStorage.getItem(Nl(e));if(!t)return[];let n=JSON.parse(t),o=Date.now()-10080*60*1e3;return n.filter(r=>!r.timestamp||r.timestamp>o)}catch{return[]}}function Mm(e,t){if(!(typeof window>"u"))try{localStorage.setItem(Nl(e),JSON.stringify(t))}catch{try{localStorage.setItem(Nl(e),JSON.stringify(Bw(t)))}catch{}}}function Bw(e){return e.map(t=>{if(!t||typeof t!="object"||!("screenshot"in t))return t;let{screenshot:n,...o}=t;return o})}function By(){let e=new Map;if(typeof window>"u")return e;try{let t=Date.now()-6048e5;for(let n=0;n<localStorage.length;n++){let o=localStorage.key(n);if(o?.startsWith(Cm)){let r=o.slice(Cm.length),i=localStorage.getItem(o);if(i){let l=JSON.parse(i).filter(a=>!a.timestamp||a.timestamp>t);l.length>0&&e.set(r,l)}}}}catch{}return e}function qc(e,t,n){let o=t.map(r=>({...r,_syncedTo:n}));Mm(e,o)}var Tm="agentation-design-";function zy(e){if(typeof window>"u")return[];try{let t=localStorage.getItem(`${Tm}${e}`);return t?JSON.parse(t):[]}catch{return[]}}function Hy(e,t){if(!(typeof window>"u"))try{localStorage.setItem(`${Tm}${e}`,JSON.stringify(t))}catch{}}function Wy(e){if(!(typeof window>"u"))try{localStorage.removeItem(`${Tm}${e}`)}catch{}}var Lm="agentation-rearrange-";function Uy(e){if(typeof window>"u")return null;try{let t=localStorage.getItem(`${Lm}${e}`);return t?JSON.parse(t):null}catch{return null}}function jy(e,t){if(!(typeof window>"u"))try{localStorage.setItem(`${Lm}${e}`,JSON.stringify(t))}catch{}}function Yy(e){if(!(typeof window>"u"))try{localStorage.removeItem(`${Lm}${e}`)}catch{}}var Rm="agentation-wireframe-";function Vy(e){if(typeof window>"u")return null;try{let t=localStorage.getItem(`${Rm}${e}`);return t?JSON.parse(t):null}catch{return null}}function Pm(e,t){if(!(typeof window>"u"))try{localStorage.setItem(`${Rm}${e}`,JSON.stringify(t))}catch{}}function Il(e){if(!(typeof window>"u"))try{localStorage.removeItem(`${Rm}${e}`)}catch{}}var Xy="agentation-session-";function Nm(e){return`${Xy}${e}`}function Qy(e){if(typeof window>"u")return null;try{return localStorage.getItem(Nm(e))}catch{return null}}function Kc(e,t){if(!(typeof window>"u"))try{localStorage.setItem(Nm(e),t)}catch{}}function Gy(e){if(!(typeof window>"u"))try{localStorage.removeItem(Nm(e))}catch{}}var Em=`${Xy}toolbar-hidden`;function qy(){if(typeof window>"u")return!1;try{return sessionStorage.getItem(Em)==="1"}catch{return!1}}function Ky(e){if(!(typeof window>"u"))try{e?sessionStorage.setItem(Em,"1"):sessionStorage.removeItem(Em)}catch{}}async function Jc(e,t){let n=await fetch(`${e}/sessions`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:t})});if(!n.ok)throw new Error(`Failed to create session: ${n.status}`);return n.json()}async function Im(e,t){let n=await fetch(`${e}/sessions/${t}`);if(!n.ok)throw new Error(`Failed to get session: ${n.status}`);return n.json()}async function Dm(e,t,n){!n.elementPath&&(n.element==="body"||n.element==="html")&&(n={...n,elementPath:n.element});let o=await fetch(`${e}/sessions/${t}/annotations`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(n)});if(!o.ok)throw new Error(`Failed to sync annotation: ${o.status}`);return o.json()}async function Zc(e,t,n){let o=await fetch(`${e}/annotations/${t}`,{method:"PATCH",headers:{"Content-Type":"application/json"},body:JSON.stringify(n)});if(!o.ok)throw new Error(`Failed to update annotation: ${o.status}`);return o.json()}async function Dl(e,t){let n=await fetch(`${e}/annotations/${t}`,{method:"DELETE"});if(!n.ok)throw new Error(`Failed to delete annotation: ${n.status}`)}var ct={FunctionComponent:0,ClassComponent:1,IndeterminateComponent:2,HostRoot:3,HostPortal:4,HostComponent:5,HostText:6,Fragment:7,Mode:8,ContextConsumer:9,ContextProvider:10,ForwardRef:11,Profiler:12,SuspenseComponent:13,MemoComponent:14,SimpleMemoComponent:15,LazyComponent:16,IncompleteClassComponent:17,DehydratedFragment:18,SuspenseListComponent:19,ScopeComponent:21,OffscreenComponent:22,LegacyHiddenComponent:23,CacheComponent:24,TracingMarkerComponent:25,HostHoistable:26,HostSingleton:27,IncompleteFunctionComponent:28,Throw:29,ViewTransitionComponent:30,ActivityComponent:31},Jy=new Set(["Component","PureComponent","Fragment","Suspense","Profiler","StrictMode","Routes","Route","Outlet","Root","ErrorBoundaryHandler","HotReload","Hot"]),Zy=[/Boundary$/,/BoundaryHandler$/,/Provider$/,/Consumer$/,/^(Inner|Outer)/,/Router$/,/^Client(Page|Segment|Root)/,/^Segment(ViewNode|Node)$/,/^LayoutSegment/,/^Server(Root|Component|Render)/,/^RSC/,/Context$/,/^Hot(Reload)?$/,/^(Dev|React)(Overlay|Tools|Root)/,/Overlay$/,/Handler$/,/^With[A-Z]/,/Wrapper$/,/^Root$/],zw=[/Page$/,/View$/,/Screen$/,/Section$/,/Card$/,/List$/,/Item$/,/Form$/,/Modal$/,/Dialog$/,/Button$/,/Nav$/,/Header$/,/Footer$/,/Layout$/,/Panel$/,/Tab$/,/Menu$/];function Hw(e){let t=e?.mode??"filtered",n=Jy;if(e?.skipExact){let o=e.skipExact instanceof Set?e.skipExact:new Set(e.skipExact);n=new Set([...Jy,...o])}return{maxComponents:e?.maxComponents??6,maxDepth:e?.maxDepth??30,mode:t,skipExact:n,skipPatterns:e?.skipPatterns?[...Zy,...e.skipPatterns]:Zy,userPatterns:e?.userPatterns??zw,filter:e?.filter}}function Ww(e){return e.replace(/([a-z])([A-Z])/g,"$1-$2").replace(/([A-Z])([A-Z][a-z])/g,"$1-$2").toLowerCase()}function Uw(e,t=10){let n=new Set,o=e,r=0;for(;o&&r<t;)o.className&&typeof o.className=="string"&&o.className.split(/\s+/).forEach(i=>{if(i.length>1){let s=i.replace(/[_][a-zA-Z0-9]{5,}.*$/,"").toLowerCase();s.length>1&&n.add(s)}}),o=o.parentElement,r++;return n}function jw(e,t){let n=Ww(e);for(let o of t){if(o===n)return!0;let r=n.split("-").filter(s=>s.length>2),i=o.split("-").filter(s=>s.length>2);for(let s of r)for(let l of i)if(s===l||s.includes(l)||l.includes(s))return!0}return!1}function Yw(e,t,n,o){if(n.filter)return n.filter(e,t);switch(n.mode){case"all":return!0;case"filtered":return!(n.skipExact.has(e)||n.skipPatterns.some(r=>r.test(e)));case"smart":return n.skipExact.has(e)||n.skipPatterns.some(r=>r.test(e))?!1:!!(o&&jw(e,o)||n.userPatterns.some(r=>r.test(e)));default:return!0}}var as=null,Vw=new WeakMap;function $m(e){return Object.keys(e).some(t=>t.startsWith("__reactFiber$")||t.startsWith("__reactInternalInstance$")||t.startsWith("__reactProps$"))}function Xw(){if(as!==null)return as;if(typeof document>"u")return!1;if(document.body&&$m(document.body))return as=!0,!0;let e=["#root","#app","#__next","[data-reactroot]"];for(let t of e){let n=document.querySelector(t);if(n&&$m(n))return as=!0,!0}if(document.body){for(let t of document.body.children)if($m(t))return as=!0,!0}return as=!1,!1}var $l={map:Vw};function Qw(e){return Object.keys(e).find(n=>n.startsWith("__reactFiber$")||n.startsWith("__reactInternalInstance$"))||null}function Gw(e){let t=Qw(e);return t?e[t]:null}function li(e){return e?e.displayName?e.displayName:e.name?e.name:null:null}function qw(e){let{tag:t,type:n,elementType:o}=e;if(t===ct.HostComponent||t===ct.HostText||t===ct.HostHoistable||t===ct.HostSingleton||t===ct.Fragment||t===ct.Mode||t===ct.Profiler||t===ct.DehydratedFragment||t===ct.HostRoot||t===ct.HostPortal||t===ct.ScopeComponent||t===ct.OffscreenComponent||t===ct.LegacyHiddenComponent||t===ct.CacheComponent||t===ct.TracingMarkerComponent||t===ct.Throw||t===ct.ViewTransitionComponent||t===ct.ActivityComponent)return null;if(t===ct.ForwardRef){let r=o;if(r?.render){let i=li(r.render);if(i)return i}return r?.displayName?r.displayName:li(n)}if(t===ct.MemoComponent||t===ct.SimpleMemoComponent){let r=o;if(r?.type){let i=li(r.type);if(i)return i}return r?.displayName?r.displayName:li(n)}if(t===ct.ContextProvider){let r=n;return r?._context?.displayName?`${r._context.displayName}.Provider`:null}if(t===ct.ContextConsumer){let r=n;return r?.displayName?`${r.displayName}.Consumer`:null}if(t===ct.LazyComponent){let r=o;return r?._status===1&&r._result?li(r._result):null}return t===ct.SuspenseComponent||t===ct.SuspenseListComponent?null:t===ct.IncompleteClassComponent||t===ct.IncompleteFunctionComponent||t===ct.FunctionComponent||t===ct.ClassComponent||t===ct.IndeterminateComponent?li(n):null}function Kw(e){return e.length<=2||e.length<=3&&e===e.toLowerCase()}function e1(e,t){let n=Hw(t),o=n.mode==="all";if(o){let a=$l.map.get(e);if(a!==void 0)return a}if(!Xw()){let a={path:null,components:[]};return o&&$l.map.set(e,a),a}let r=n.mode==="smart"?Uw(e):void 0,i=[];try{let a=Gw(e),c=0;for(;a&&c<n.maxDepth&&i.length<n.maxComponents;){let d=qw(a);d&&!Kw(d)&&Yw(d,c,n,r)&&i.push(d),a=a.return,c++}}catch{let a={path:null,components:[]};return o&&$l.map.set(e,a),a}if(i.length===0){let a={path:null,components:[]};return o&&$l.map.set(e,a),a}let l={path:i.slice().reverse().map(a=>`<${a}>`).join(" "),components:i};return o&&$l.map.set(e,l),l}var Jw=ce(vt()),Al={FunctionComponent:0,ClassComponent:1,IndeterminateComponent:2,HostRoot:3,HostPortal:4,HostComponent:5,HostText:6,Fragment:7,Mode:8,ContextConsumer:9,ContextProvider:10,ForwardRef:11,Profiler:12,SuspenseComponent:13,MemoComponent:14,SimpleMemoComponent:15,LazyComponent:16};function Zw(e){if(!e||typeof e!="object")return null;let t=Object.keys(e),n=t.find(i=>i.startsWith("__reactFiber$"));if(n)return e[n]||null;let o=t.find(i=>i.startsWith("__reactInternalInstance$"));if(o)return e[o]||null;let r=t.find(i=>{if(!i.startsWith("__react"))return!1;let s=e[i];return s&&typeof s=="object"&&"_debugSource"in s});return r&&e[r]||null}function Fl(e){if(!e.type||typeof e.type=="string")return null;if(typeof e.type=="object"||typeof e.type=="function"){let t=e.type;if(t.displayName)return t.displayName;if(t.name)return t.name}return null}function eb(e,t=50){let n=e,o=0;for(;n&&o<t;){if(n._debugSource)return{source:n._debugSource,componentName:Fl(n)};if(n._debugOwner?._debugSource)return{source:n._debugOwner._debugSource,componentName:Fl(n._debugOwner)};n=n.return,o++}return null}function tb(e){let t=e,n=0,o=50;for(;t&&n<o;){let r=t,i=["_debugSource","__source","_source","debugSource"];for(let s of i){let l=r[s];if(l&&typeof l=="object"&&"fileName"in l)return{source:l,componentName:Fl(t)}}if(t.memoizedProps){let s=t.memoizedProps;if(s.__source&&typeof s.__source=="object"){let l=s.__source;if(l.fileName&&l.lineNumber)return{source:{fileName:l.fileName,lineNumber:l.lineNumber,columnNumber:l.columnNumber},componentName:Fl(t)}}}t=t.return,n++}return null}var eu=new Map;function nb(e){let t=e.tag,n=e.type,o=e.elementType;if(typeof n=="string"||n==null||typeof n=="function"&&n.prototype?.isReactComponent)return null;if((t===Al.FunctionComponent||t===Al.IndeterminateComponent)&&typeof n=="function")return n;if(t===Al.ForwardRef&&o){let r=o.render;if(typeof r=="function")return r}if((t===Al.MemoComponent||t===Al.SimpleMemoComponent)&&o){let r=o.type;if(typeof r=="function")return r}return typeof n=="function"?n:null}function ob(){let e=Jw,t=e.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;if(t&&"H"in t)return{get:()=>t.H,set:o=>{t.H=o}};let n=e.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED;if(n){let o=n.ReactCurrentDispatcher;if(o&&"current"in o)return{get:()=>o.current,set:r=>{o.current=r}}}return null}function rb(e,t){let n=e.split(`
`),o=[/source-location/,/\/dist\/index\./,/node_modules\//,/react-dom/,/react\.development/,/react\.production/,/chunk-[A-Z0-9]+/i,/\/_next\/static\/chunks\//,/\/\.vite\/deps\//,/\/_astro\//,/\/assets\/[^\s/]+[-.][\w-]{8,}\.m?js(?:[?:]|$)/,/react-stack-bottom-frame/,/react-reconciler/,/scheduler/,/<anonymous>/],r=/^\s*at\s+(?:.*?\s+\()?(.+?):(\d+):(\d+)\)?$/,i=/^[^@]*@(.+?):(\d+):(\d+)$/;for(let s of n){let l=s.trim();if(!l||o.some(c=>c.test(l)))continue;if(t){let c=t.replace(/^bound /,"").replace(/[.*+?^${}()|[\]\\]/g,"\\$&");if(!new RegExp(`(?:at (?:Object\\.)?|^)${c}(?: \\(|@| \\[)`).test(l))continue}let a=r.exec(l)||i.exec(l);if(a)return{fileName:a[1],line:parseInt(a[2],10),column:parseInt(a[3],10)}}return null}function ib(e){let t=e;return t=t.replace(/[?#].*$/,""),t=t.replace(/^turbopack:\/\/\/\[project\]\//,""),t=t.replace(/^webpack-internal:\/\/\/\.\//,""),t=t.replace(/^webpack-internal:\/\/\//,""),t=t.replace(/^webpack:\/\/\/\.\//,""),t=t.replace(/^webpack:\/\/\//,""),t=t.replace(/^turbopack:\/\/\//,""),t=t.replace(/^https?:\/\/[^/]+\//,""),t=t.replace(/^file:\/\/\//,"/"),t=t.replace(/^\([^)]+\)\/\.\//,""),t=t.replace(/^\.\//,""),t}function sb(e){let t=nb(e);if(!t)return null;if(eu.has(t))return eu.get(t);let n=ob();if(!n)return eu.set(t,null),null;let o=n.get(),r=null;try{let i=new Proxy({},{get(){throw new Error("probe")}});n.set(i);try{t({})}catch(s){if(s instanceof Error&&s.message==="probe"&&s.stack){let l=rb(s.stack,t.name);l&&(r={fileName:ib(l.fileName),lineNumber:l.line,columnNumber:l.column,componentName:Fl(e)||void 0})}}}finally{n.set(o)}return eu.set(t,r),r}function lb(e,t=15){let n=e,o=0;for(;n&&o<t;){let r=sb(n);if(r)return r;n=n.return,o++}return null}function tu(e){let t=Zw(e);if(!t)return{found:!1,reason:"no-fiber",isReactApp:!1,isProduction:!1};let n=eb(t);if(n||(n=tb(t)),n?.source)return{found:!0,source:{fileName:n.source.fileName,lineNumber:n.source.lineNumber,columnNumber:n.source.columnNumber,componentName:n.componentName||void 0},isReactApp:!0,isProduction:!1};let o=lb(t);return o?{found:!0,source:o,isReactApp:!0,isProduction:!1}:{found:!1,reason:"no-debug-source",isReactApp:!0,isProduction:!1}}function t1(e,t="path"){let{fileName:n,lineNumber:o,columnNumber:r}=e,i=`${n}:${o}`;return r!==void 0&&(i+=`:${r}`),t==="vscode"?`vscode://file${n.startsWith("/")?"":"/"}${i}`:i}function n1(e,t=10){let n=e,o=0;for(;n&&o<t;){let r=tu(n);if(r.found)return r;n=n.parentElement,o++}return tu(e)}var cs=[{value:"compact",label:"Compact"},{value:"standard",label:"Standard"},{value:"detailed",label:"Detailed"},{value:"forensic",label:"Forensic"}];function Ol(e,t){let n=`## Page Feedback: ${e}
`,o=t?.replace(/[\r\n\t]+/g," ").trim();return o&&(n+=`**App:** ${o.replace(/[\\`*_\[\]<>]/g,"\\$&")}
`),n}function Am(e,t,n="standard",o={}){if(e.length===0)return"";let r=typeof window<"u"?`${window.innerWidth}\xD7${window.innerHeight}`:"unknown",i=Ol(t,o.appName);n==="forensic"?(i+=`
**Environment:**
`,i+=`- Viewport: ${r}
`,typeof window<"u"&&(i+=`- URL: ${window.location.href}
`,i+=`- User Agent: ${navigator.userAgent}
`,i+=`- Timestamp: ${new Date().toISOString()}
`,i+=`- Device Pixel Ratio: ${window.devicePixelRatio}
`),i+=`
---
`):n!=="compact"&&(i+=`**Viewport:** ${r}
`),i+=`
`;let s=0,l=a=>{if(!a.screenshot||!o.screenshotReference)return;let c=o.screenshotReference(a,s+1);if(c)return s+=1,c};return e.forEach((a,c)=>{let d=l(a);n==="compact"?(i+=`${c+1}. **${a.element}**${a.sourceFile?` (${a.sourceFile})`:""}: ${a.comment}`,a.selectedText&&(i+=` (re: "${a.selectedText.slice(0,30)}${a.selectedText.length>30?"...":""}")`),d&&(i+=` ${d}`),i+=`
`):n==="forensic"?(i+=`### ${c+1}. ${a.element}
`,a.isMultiSelect&&a.fullPath&&(i+=`*Forensic data shown for first element of selection*
`),a.fullPath&&(i+=`**Full DOM Path:** ${a.fullPath}
`),a.cssClasses&&(i+=`**CSS Classes:** ${a.cssClasses}
`),a.boundingBox&&(i+=`**Position:** x:${Math.round(a.boundingBox.x)}, y:${Math.round(a.boundingBox.y)} (${Math.round(a.boundingBox.width)}\xD7${Math.round(a.boundingBox.height)}px)
`),i+=`**Annotation at:** ${a.x.toFixed(1)}% from left, ${Math.round(a.y)}px from top
`,a.selectedText&&(i+=`**Selected text:** "${a.selectedText}"
`),a.nearbyText&&!a.selectedText&&(i+=`**Context:** ${a.nearbyText.slice(0,100)}
`),a.computedStyles&&(i+=`**Computed Styles:** ${a.computedStyles}
`),a.accessibility&&(i+=`**Accessibility:** ${a.accessibility}
`),a.nearbyElements&&(i+=`**Nearby Elements:** ${a.nearbyElements}
`),a.sourceFile&&(i+=`**Source:** ${a.sourceFile}
`),a.reactComponents&&(i+=`**React:** ${a.reactComponents}
`),d&&(i+=`**Screenshot:** ${d}
`),i+=`**Feedback:** ${a.comment}

`):(i+=`### ${c+1}. ${a.element}
`,i+=`**Location:** ${a.elementPath}
`,a.sourceFile&&(i+=`**Source:** ${a.sourceFile}
`),a.reactComponents&&(i+=`**React:** ${a.reactComponents}
`),n==="detailed"&&(a.cssClasses&&(i+=`**Classes:** ${a.cssClasses}
`),a.boundingBox&&(i+=`**Position:** ${Math.round(a.boundingBox.x)}px, ${Math.round(a.boundingBox.y)}px (${Math.round(a.boundingBox.width)}\xD7${Math.round(a.boundingBox.height)}px)
`)),a.selectedText&&(i+=`**Selected text:** "${a.selectedText}"
`),n==="detailed"&&a.nearbyText&&!a.selectedText&&(i+=`**Context:** ${a.nearbyText.slice(0,100)}
`),d&&(i+=`**Screenshot:** ${d}
`),i+=`**Feedback:** ${a.comment}

`)}),i.trim()}var c1="[modern-screenshot]",kr=typeof window<"u",ab=kr&&"Worker"in window,m6=kr&&"atob"in window,p6=kr&&"btoa"in window,Bm=kr?window.navigator?.userAgent:"",u1=Bm.includes("Chrome"),nu=Bm.includes("AppleWebKit")&&!u1,zm=Bm.includes("Firefox"),cb=e=>e&&"__CONTEXT__"in e,ub=e=>e.constructor.name==="CSSFontFaceRule",db=e=>e.constructor.name==="CSSImportRule",_b=e=>e.constructor.name==="CSSLayerBlockRule",Po=e=>e.nodeType===1,Wl=e=>typeof e.className=="object",d1=e=>e.tagName==="image",mb=e=>e.tagName==="use",Bl=e=>Po(e)&&typeof e.style<"u"&&!Wl(e),pb=e=>e.nodeType===8,fb=e=>e.nodeType===3,ds=e=>e.tagName==="IMG",ou=e=>e.tagName==="VIDEO",gb=e=>e.tagName==="CANVAS",hb=e=>e.tagName==="TEXTAREA",yb=e=>e.tagName==="INPUT",xb=e=>e.tagName==="STYLE",vb=e=>e.tagName==="SCRIPT",wb=e=>e.tagName==="SELECT",bb=e=>e.tagName==="SLOT",kb=e=>e.tagName==="IFRAME",Sb=(...e)=>console.warn(c1,...e);function Cb(e){let t=e?.createElement?.("canvas");return t&&(t.height=t.width=1),!!t&&"toDataURL"in t&&!!t.toDataURL("image/webp").includes("image/webp")}var Fm=e=>e.startsWith("data:");function _1(e,t){if(e.match(/^[a-z]+:\/\//i))return e;if(kr&&e.match(/^\/\//))return window.location.protocol+e;if(e.match(/^[a-z]+:/i)||!kr)return e;let n=ru().implementation.createHTMLDocument(),o=n.createElement("base"),r=n.createElement("a");return n.head.appendChild(o),n.body.appendChild(r),t&&(o.href=t),r.href=e,r.href}function ru(e){return(e&&Po(e)?e?.ownerDocument:e)??window.document}var iu="http://www.w3.org/2000/svg";function Eb(e,t,n){let o=ru(n).createElementNS(iu,"svg");return o.setAttributeNS(null,"width",e.toString()),o.setAttributeNS(null,"height",t.toString()),o.setAttributeNS(null,"viewBox",`0 0 ${e} ${t}`),o}function Mb(e,t){let n=new XMLSerializer().serializeToString(e);return t&&(n=n.replace(/[\u0000-\u0008\v\f\u000E-\u001F\uD800-\uDFFF\uFFFE\uFFFF]/gu,"")),`data:image/svg+xml;charset=utf-8,${encodeURIComponent(n)}`}function Tb(e,t){return new Promise((n,o)=>{let r=new FileReader;r.onload=()=>n(r.result),r.onerror=()=>o(r.error),r.onabort=()=>o(new Error(`Failed read blob to ${t}`)),t==="dataUrl"?r.readAsDataURL(e):t==="arrayBuffer"&&r.readAsArrayBuffer(e)})}var Lb=e=>Tb(e,"dataUrl");function us(e,t){let n=ru(t).createElement("img");return n.decoding="sync",n.loading="eager",n.src=e,n}function zl(e,t){return new Promise(n=>{let{timeout:o,ownerDocument:r,onError:i,onWarn:s}=t??{},l=typeof e=="string"?us(e,ru(r)):e,a=null,c=null;function d(){n(l),a&&clearTimeout(a),c?.()}if(o&&(a=setTimeout(d,o)),ou(l)){let m=l.currentSrc||l.src;if(!m)return l.poster?zl(l.poster,t).then(n):d();if(l.readyState>=2)return d();let f=d,k=w=>{s?.("Failed video load",m,w),i?.(w),d()};c=()=>{l.removeEventListener("loadeddata",f),l.removeEventListener("error",k)},l.addEventListener("loadeddata",f,{once:!0}),l.addEventListener("error",k,{once:!0})}else{let m=d1(l)?l.href.baseVal:l.currentSrc||l.src;if(!m)return d();let f=async()=>{if(ds(l)&&"decode"in l)try{await l.decode()}catch(w){s?.("Failed to decode image, trying to render anyway",l.dataset.originalSrc||m,w)}d()},k=w=>{s?.("Failed image load",l.dataset.originalSrc||m,w),d()};if(ds(l)&&l.complete)return f();c=()=>{l.removeEventListener("load",f),l.removeEventListener("error",k)},l.addEventListener("load",f,{once:!0}),l.addEventListener("error",k,{once:!0})}})}async function Rb(e,t){Bl(e)&&(ds(e)||ou(e)?await zl(e,t):await Promise.all(["img","video"].flatMap(n=>Array.from(e.querySelectorAll(n)).map(o=>zl(o,t)))))}var m1=(function(){let t=0,n=()=>`0000${(Math.random()*36**4<<0).toString(36)}`.slice(-4);return()=>(t+=1,`u${n()}${t}`)})();function p1(e){return e?.split(",").map(t=>t.trim().replace(/"|'/g,"").toLowerCase()).filter(Boolean)}var o1=0;function Pb(e){let t=`${c1}[#${o1}]`;return o1++,{time:n=>e&&console.time(`${t} ${n}`),timeEnd:n=>e&&console.timeEnd(`${t} ${n}`),warn:(...n)=>e&&Sb(...n)}}function Nb(e){return{cache:e?"no-cache":"force-cache"}}async function f1(e,t){return cb(e)?e:Ib(e,{...t,autoDestruct:!0})}async function Ib(e,t){let{scale:n=1,workerUrl:o,workerNumber:r=1}=t||{},i=!!t?.debug,s=t?.features??!0,l=e.ownerDocument??(kr?window.document:void 0),a=e.ownerDocument?.defaultView??(kr?window:void 0),c=new Map,d={width:0,height:0,quality:1,type:"image/png",scale:n,backgroundColor:null,style:null,filter:null,maximumCanvasSize:0,timeout:3e4,progress:null,debug:i,fetch:{requestInit:Nb(t?.fetch?.bypassingCache),placeholderImage:"data:image/png;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7",bypassingCache:!1,...t?.fetch},fetchFn:null,font:{},drawImageInterval:100,workerUrl:null,workerNumber:r,onCloneEachNode:null,onCloneNode:null,onEmbedNode:null,onCreateForeignObjectSvg:null,includeStyleProperties:null,autoDestruct:!1,...t,__CONTEXT__:!0,log:Pb(i),node:e,ownerDocument:l,ownerWindow:a,dpi:n===1?null:96*n,svgStyleElement:g1(l),svgDefsElement:l?.createElementNS(iu,"defs"),svgStyles:new Map,defaultComputedStyles:new Map,workers:[...Array.from({length:ab&&o&&r?r:0})].map(()=>{try{let k=new Worker(o);return k.onmessage=async w=>{let{url:C,result:S}=w.data;S?c.get(C)?.resolve?.(S):c.get(C)?.reject?.(new Error(`Error receiving message from worker: ${C}`))},k.onmessageerror=w=>{let{url:C}=w.data;c.get(C)?.reject?.(new Error(`Error receiving message from worker: ${C}`))},k}catch(k){return d.log.warn("Failed to new Worker",k),null}}).filter(Boolean),fontFamilies:new Map,fontCssTexts:new Map,acceptOfImage:`${[Cb(l)&&"image/webp","image/svg+xml","image/*","*/*"].filter(Boolean).join(",")};q=0.8`,requests:c,drawImageCount:0,tasks:[],features:s,isEnable:k=>k==="restoreScrollPosition"?typeof s=="boolean"?!1:s[k]??!1:typeof s=="boolean"?s:s[k]??!0,shadowRoots:[]};d.log.time("wait until load"),await Rb(e,{timeout:d.timeout,onWarn:d.log.warn}),d.log.timeEnd("wait until load");let{width:m,height:f}=Db(e,d);return d.width=m,d.height=f,d}function g1(e){if(!e)return;let t=e.createElement("style"),n=t.ownerDocument.createTextNode(`
.______background-clip--text {
  background-clip: text;
  -webkit-background-clip: text;
}
`);return t.appendChild(n),t}function Db(e,t){let{width:n,height:o}=t;if(Po(e)&&(!n||!o)){let r=e.getBoundingClientRect();n=n||r.width||Number(e.getAttribute("width"))||0,o=o||r.height||Number(e.getAttribute("height"))||0}return{width:n,height:o}}async function $b(e,t){let{log:n,timeout:o,drawImageCount:r,drawImageInterval:i}=t;n.time("image to canvas");let s=await zl(e,{timeout:o,onWarn:t.log.warn}),{canvas:l,context2d:a}=Ab(e.ownerDocument,t),c=()=>{try{a?.drawImage(s,0,0,l.width,l.height)}catch(d){t.log.warn("Failed to drawImage",d)}};if(c(),t.isEnable("fixSvgXmlDecode"))for(let d=0;d<r;d++)await new Promise(m=>{setTimeout(()=>{a?.clearRect(0,0,l.width,l.height),c(),m()},d+i)});return t.drawImageCount=0,n.timeEnd("image to canvas"),l}function Ab(e,t){let{width:n,height:o,scale:r,backgroundColor:i,maximumCanvasSize:s}=t,l=e.createElement("canvas");l.width=Math.floor(n*r),l.height=Math.floor(o*r),l.style.width=`${n}px`,l.style.height=`${o}px`,s&&(l.width>s||l.height>s)&&(l.width>s&&l.height>s?l.width>l.height?(l.height*=s/l.width,l.width=s):(l.width*=s/l.height,l.height=s):l.width>s?(l.height*=s/l.width,l.width=s):(l.width*=s/l.height,l.height=s));let a=l.getContext("2d");return a&&i&&(a.fillStyle=i,a.fillRect(0,0,l.width,l.height)),{canvas:l,context2d:a}}function h1(e,t){if(e.ownerDocument)try{let i=e.toDataURL();if(i!=="data:,")return us(i,e.ownerDocument)}catch(i){t.log.warn("Failed to clone canvas",i)}let n=e.cloneNode(!1),o=e.getContext("2d"),r=n.getContext("2d");try{return o&&r&&r.putImageData(o.getImageData(0,0,e.width,e.height),0,0),n}catch(i){t.log.warn("Failed to clone canvas",i)}return n}function Fb(e,t){try{if(e?.contentDocument?.documentElement)return Hm(e.contentDocument.documentElement,t)}catch(n){t.log.warn("Failed to clone iframe",n)}return e.cloneNode(!1)}function Ob(e){let t=e.cloneNode(!1);return e.currentSrc&&e.currentSrc!==e.src&&(t.src=e.currentSrc,t.srcset=""),t.loading==="lazy"&&(t.loading="eager"),t}async function Bb(e,t){if(e.ownerDocument&&!e.currentSrc&&e.poster)return us(e.poster,e.ownerDocument);let n=e.cloneNode(!1);n.crossOrigin="anonymous",e.currentSrc&&e.currentSrc!==e.src&&(n.src=e.currentSrc);let o=n.ownerDocument;if(o){let r=!0;if(await zl(n,{onError:()=>r=!1,onWarn:t.log.warn}),!r)return e.poster?us(e.poster,e.ownerDocument):n;n.currentTime=e.currentTime,await new Promise(s=>{n.addEventListener("seeked",s,{once:!0})});let i=o.createElement("canvas");i.width=e.offsetWidth,i.height=e.offsetHeight;try{let s=i.getContext("2d");s&&s.drawImage(n,0,0,i.width,i.height)}catch(s){return t.log.warn("Failed to clone video",s),e.poster?us(e.poster,e.ownerDocument):n}return h1(i,t)}return n}function zb(e,t){return gb(e)?h1(e,t):kb(e)?Fb(e,t):ds(e)?Ob(e):ou(e)?Bb(e,t):e.cloneNode(!1)}function Hb(e){let t=e.sandbox;if(!t){let{ownerDocument:n}=e;try{n&&(t=n.createElement("iframe"),t.id=`__SANDBOX__${m1()}`,t.width="0",t.height="0",t.style.visibility="hidden",t.style.position="fixed",n.body.appendChild(t),t.srcdoc='<!DOCTYPE html><meta charset="UTF-8"><title></title><body>',e.sandbox=t)}catch(o){e.log.warn("Failed to getSandBox",o)}}return t}var Wb=["width","height","-webkit-text-fill-color"],Ub=["stroke","fill"];function y1(e,t,n){let{defaultComputedStyles:o}=n,r=e.nodeName.toLowerCase(),i=Wl(e)&&r!=="svg",s=i?Ub.map(C=>[C,e.getAttribute(C)]).filter(([,C])=>C!==null):[],l=[i&&"svg",r,s.map((C,S)=>`${C}=${S}`).join(","),t].filter(Boolean).join(":");if(o.has(l))return o.get(l);let c=Hb(n)?.contentWindow;if(!c)return new Map;let d=c?.document,m,f;i?(m=d.createElementNS(iu,"svg"),f=m.ownerDocument.createElementNS(m.namespaceURI,r),s.forEach(([C,S])=>{f.setAttributeNS(null,C,S)}),m.appendChild(f)):m=f=d.createElement(r),f.textContent=" ",d.body.appendChild(m);let k=c.getComputedStyle(f,t),w=new Map;for(let C=k.length,S=0;S<C;S++){let g=k.item(S);Wb.includes(g)||w.set(g,k.getPropertyValue(g))}return d.body.removeChild(m),o.set(l,w),w}function x1(e,t,n){let o=new Map,r=[],i=new Map;if(n)for(let l of n)s(l);else for(let l=e.length,a=0;a<l;a++){let c=e.item(a);s(c)}for(let l=r.length,a=0;a<l;a++)i.get(r[a])?.forEach((c,d)=>o.set(d,c));function s(l){let a=e.getPropertyValue(l),c=e.getPropertyPriority(l),d=l.lastIndexOf("-"),m=d>-1?l.substring(0,d):void 0;if(m){let f=i.get(m);f||(f=new Map,i.set(m,f)),f.set(l,[a,c])}t.get(l)===a&&!c||(m?r.push(m):o.set(l,[a,c]))}return o}function jb(e,t,n,o){let{ownerWindow:r,includeStyleProperties:i,currentParentNodeStyle:s}=o,l=t.style,a=r.getComputedStyle(e),c=y1(e,null,o);s?.forEach((m,f)=>{c.delete(f)});let d=x1(a,c,i);d.delete("transition-property"),d.delete("all"),d.delete("d"),d.delete("content"),n&&(d.delete("position"),d.delete("margin-top"),d.delete("margin-right"),d.delete("margin-bottom"),d.delete("margin-left"),d.delete("margin-block-start"),d.delete("margin-block-end"),d.delete("margin-inline-start"),d.delete("margin-inline-end"),d.set("box-sizing",["border-box",""])),d.get("background-clip")?.[0]==="text"&&t.classList.add("______background-clip--text"),u1&&(d.has("font-kerning")||d.set("font-kerning",["normal",""]),(d.get("overflow-x")?.[0]==="hidden"||d.get("overflow-y")?.[0]==="hidden")&&d.get("text-overflow")?.[0]==="ellipsis"&&e.scrollWidth===e.clientWidth&&d.set("text-overflow",["clip",""]));for(let m=l.length,f=0;f<m;f++)l.removeProperty(l.item(f));return d.forEach(([m,f],k)=>{l.setProperty(k,m,f)}),d}function Yb(e,t){(hb(e)||yb(e)||wb(e))&&t.setAttribute("value",e.value)}var Vb=["::before","::after"],Xb=["::-webkit-scrollbar","::-webkit-scrollbar-button","::-webkit-scrollbar-thumb","::-webkit-scrollbar-track","::-webkit-scrollbar-track-piece","::-webkit-scrollbar-corner","::-webkit-resizer"];function Qb(e,t,n,o,r){let{ownerWindow:i,svgStyleElement:s,svgStyles:l,currentNodeStyle:a}=o;if(!s||!i)return;function c(d){let m=i.getComputedStyle(e,d),f=m.getPropertyValue("content");if(!f||f==="none")return;r?.(f),f=f.replace(/(')|(")|(counter\(.+\))/g,"");let k=[m1()],w=y1(e,d,o);a?.forEach((h,E)=>{w.delete(E)});let C=x1(m,w,o.includeStyleProperties);C.delete("content"),C.delete("-webkit-locale"),C.get("background-clip")?.[0]==="text"&&t.classList.add("______background-clip--text");let S=[`content: '${f}';`];if(C.forEach(([h,E],F)=>{S.push(`${F}: ${h}${E?" !important":""};`)}),S.length===1)return;try{t.className=[t.className,...k].join(" ")}catch(h){o.log.warn("Failed to copyPseudoClass",h);return}let g=S.join(`
  `),x=l.get(g);x||(x=[],l.set(g,x)),x.push(`.${k[0]}${d}`)}Vb.forEach(c),n&&Xb.forEach(c)}var r1=new Set(["symbol"]);async function i1(e,t,n,o,r){if(Po(n)&&(xb(n)||vb(n))||o.filter&&!o.filter(n))return;r1.has(t.nodeName)||r1.has(n.nodeName)?o.currentParentNodeStyle=void 0:o.currentParentNodeStyle=o.currentNodeStyle;let i=await Hm(n,o,!1,r);o.isEnable("restoreScrollPosition")&&Gb(e,i),t.appendChild(i)}async function s1(e,t,n,o){let r=e.firstChild;Po(e)&&e.shadowRoot&&(r=e.shadowRoot?.firstChild,n.shadowRoots.push(e.shadowRoot));for(let i=r;i;i=i.nextSibling)if(!pb(i))if(Po(i)&&bb(i)&&typeof i.assignedNodes=="function"){let s=i.assignedNodes();for(let l=0;l<s.length;l++)await i1(e,t,s[l],n,o)}else await i1(e,t,i,n,o)}function Gb(e,t){if(!Bl(e)||!Bl(t))return;let{scrollTop:n,scrollLeft:o}=e;if(!n&&!o)return;let{transform:r}=t.style,i=new DOMMatrix(r),{a:s,b:l,c:a,d:c}=i;i.a=1,i.b=0,i.c=0,i.d=1,i.translateSelf(-o,-n),i.a=s,i.b=l,i.c=a,i.d=c,t.style.transform=i.toString()}function qb(e,t){let{backgroundColor:n,width:o,height:r,style:i}=t,s=e.style;if(n&&s.setProperty("background-color",n,"important"),o&&s.setProperty("width",`${o}px`,"important"),r&&s.setProperty("height",`${r}px`,"important"),i)for(let l in i)s[l]=i[l]}var Kb=/^[\w-:]+$/;async function Hm(e,t,n=!1,o){let{ownerDocument:r,ownerWindow:i,fontFamilies:s,onCloneEachNode:l}=t;if(r&&fb(e))return o&&/\S/.test(e.data)&&o(e.data),r.createTextNode(e.data);if(r&&i&&Po(e)&&(Bl(e)||Wl(e))){let c=await zb(e,t);if(t.isEnable("removeAbnormalAttributes")){let C=c.getAttributeNames();for(let S=C.length,g=0;g<S;g++){let x=C[g];Kb.test(x)||c.removeAttribute(x)}}let d=t.currentNodeStyle=jb(e,c,n,t);n&&qb(c,t);let m=!1;if(t.isEnable("copyScrollbar")){let C=[d.get("overflow-x")?.[0],d.get("overflow-y")?.[0]];m=C.includes("scroll")||(C.includes("auto")||C.includes("overlay"))&&(e.scrollHeight>e.clientHeight||e.scrollWidth>e.clientWidth)}let f=d.get("text-transform")?.[0],k=p1(d.get("font-family")?.[0]),w=k?C=>{f==="uppercase"?C=C.toUpperCase():f==="lowercase"?C=C.toLowerCase():f==="capitalize"&&(C=C[0].toUpperCase()+C.substring(1)),k.forEach(S=>{let g=s.get(S);g||s.set(S,g=new Set),C.split("").forEach(x=>g.add(x))})}:void 0;return Qb(e,c,m,t,w),Yb(e,c),ou(e)||await s1(e,c,t,w),await l?.(c),c}let a=e.cloneNode(!1);return await s1(e,a,t),await l?.(a),a}function Jb(e){if(e.ownerDocument=void 0,e.ownerWindow=void 0,e.svgStyleElement=void 0,e.svgDefsElement=void 0,e.svgStyles.clear(),e.defaultComputedStyles.clear(),e.sandbox){try{e.sandbox.remove()}catch(t){e.log.warn("Failed to destroyContext",t)}e.sandbox=void 0}e.workers=[],e.fontFamilies.clear(),e.fontCssTexts.clear(),e.requests.clear(),e.tasks=[],e.shadowRoots=[]}function Zb(e){let{url:t,timeout:n,responseType:o,...r}=e,i=new AbortController,s=n?setTimeout(()=>i.abort(),n):void 0;return fetch(t,{signal:i.signal,...r}).then(l=>{if(!l.ok)throw new Error("Failed fetch, not 2xx response",{cause:l});switch(o){case"arrayBuffer":return l.arrayBuffer();case"dataUrl":return l.blob().then(Lb);default:return l.text()}}).finally(()=>clearTimeout(s))}function Hl(e,t){let{url:n,requestType:o="text",responseType:r="text",imageDom:i}=t,s=n,{timeout:l,acceptOfImage:a,requests:c,fetchFn:d,fetch:{requestInit:m,bypassingCache:f,placeholderImage:k},font:w,workers:C,fontFamilies:S}=e;o==="image"&&(nu||zm)&&e.drawImageCount++;let g=c.get(n);if(!g){f&&f instanceof RegExp&&f.test(s)&&(s+=(/\?/.test(s)?"&":"?")+new Date().getTime());let x=o.startsWith("font")&&w&&w.minify,h=new Set;x&&o.split(";")[1].split(",").forEach(D=>{S.has(D)&&S.get(D).forEach(j=>h.add(j))});let E=x&&h.size,F={url:s,timeout:l,responseType:E?"arrayBuffer":r,headers:o==="image"?{accept:a}:void 0,...m};g={type:o,resolve:void 0,reject:void 0,response:null},g.response=(async()=>{if(d&&o==="image"){let W=await d(n);if(W)return W}return!nu&&n.startsWith("http")&&C.length?new Promise((W,D)=>{C[c.size&C.length-1].postMessage({rawUrl:n,...F}),g.resolve=W,g.reject=D}):Zb(F)})().catch(W=>{if(c.delete(n),o==="image"&&k)return e.log.warn("Failed to fetch image base64, trying to use placeholder image",s),typeof k=="string"?k:k(i);throw W}),c.set(n,g)}return g.response}async function v1(e,t,n,o){if(!w1(e))return e;for(let[r,i]of ek(e,t))try{let s=await Hl(n,{url:i,requestType:o?"image":"text",responseType:"dataUrl"});e=e.replace(tk(r),`$1${s}$3`)}catch(s){n.log.warn("Failed to fetch css data url",r,s)}return e}function w1(e){return/url\((['"]?)([^'"]+?)\1\)/.test(e)}var b1=/url\((['"]?)([^'"]+?)\1\)/g;function ek(e,t){let n=[];return e.replace(b1,(o,r,i)=>(n.push([i,_1(i,t)]),o)),n.filter(([o])=>!Fm(o))}function tk(e){let t=e.replace(/([.*+?^${}()|\[\]\/\\])/g,"\\$1");return new RegExp(`(url\\(['"]?)(${t})(['"]?\\))`,"g")}var nk=["background-image","border-image-source","-webkit-border-image","-webkit-mask-image","list-style-image"];function ok(e,t){return nk.map(n=>{let o=e.getPropertyValue(n);return!o||o==="none"?null:((nu||zm)&&t.drawImageCount++,v1(o,null,t,!0).then(r=>{!r||o===r||e.setProperty(n,r,e.getPropertyPriority(n))}))}).filter(Boolean)}function rk(e,t){if(ds(e)){let n=e.currentSrc||e.src;if(!Fm(n))return[Hl(t,{url:n,imageDom:e,requestType:"image",responseType:"dataUrl"}).then(o=>{o&&(e.srcset="",e.dataset.originalSrc=n,e.src=o||"")})];(nu||zm)&&t.drawImageCount++}else if(Wl(e)&&!Fm(e.href.baseVal)){let n=e.href.baseVal;return[Hl(t,{url:n,imageDom:e,requestType:"image",responseType:"dataUrl"}).then(o=>{o&&(e.dataset.originalSrc=n,e.href.baseVal=o||"")})]}return[]}function ik(e,t){let{ownerDocument:n,svgDefsElement:o}=t,r=e.getAttribute("href")??e.getAttribute("xlink:href");if(!r)return[];let[i,s]=r.split("#");if(s){let l=`#${s}`,a=t.shadowRoots.reduce((c,d)=>c??d.querySelector(`svg ${l}`),n?.querySelector(`svg ${l}`));if(i&&e.setAttribute("href",l),o?.querySelector(l))return[];if(a)return o?.appendChild(a.cloneNode(!0)),[];if(i)return[Hl(t,{url:i,responseType:"text"}).then(c=>{o?.insertAdjacentHTML("beforeend",c)})]}return[]}function k1(e,t){let{tasks:n}=t;Po(e)&&((ds(e)||d1(e))&&n.push(...rk(e,t)),mb(e)&&n.push(...ik(e,t))),Bl(e)&&n.push(...ok(e.style,t)),e.childNodes.forEach(o=>{k1(o,t)})}async function sk(e,t){let{ownerDocument:n,svgStyleElement:o,fontFamilies:r,fontCssTexts:i,tasks:s,font:l}=t;if(!(!n||!o||!r.size))if(l&&l.cssText){let a=a1(l.cssText,t);o.appendChild(n.createTextNode(`${a}
`))}else{let a=Array.from(n.styleSheets).filter(k=>{try{return"cssRules"in k&&!!k.cssRules.length}catch(w){return t.log.warn(`Error while reading CSS rules from ${k.href}`,w),!1}}),c=n.implementation.createHTMLDocument(""),d=c.createElement("style");c.head.appendChild(d);let m=d.sheet;await Promise.all(a.flatMap(k=>Array.from(k.cssRules).map(async w=>{if(db(w)){let C=w.href,S="";try{S=await Hl(t,{url:C,requestType:"text",responseType:"text"})}catch(x){t.log.warn(`Error fetch remote css import from ${C}`,x)}let g=S.replace(b1,(x,h,E)=>x.replace(E,_1(E,C)));for(let x of ak(g))try{m.insertRule(x,m.cssRules.length)}catch(h){t.log.warn("Error inserting rule from remote css import",{rule:x,error:h})}}}))),m.cssRules.length&&a.push(m);let f=[];a.forEach(k=>{Om(k.cssRules,f)}),f.filter(k=>ub(k)&&w1(k.style.getPropertyValue("src"))&&p1(k.style.getPropertyValue("font-family"))?.some(w=>r.has(w))).forEach(k=>{let w=k,C=i.get(w.cssText);C?o.appendChild(n.createTextNode(`${C}
`)):s.push(v1(w.cssText,w.parentStyleSheet?w.parentStyleSheet.href:null,t).then(S=>{S=a1(S,t),i.set(w.cssText,S),o.appendChild(n.createTextNode(`${S}
`))}))})}}var lk=/(\/\*[\s\S]*?\*\/)/g,l1=/((@.*?keyframes [\s\S]*?){([\s\S]*?}\s*?)})/gi;function ak(e){if(e==null)return[];let t=[],n=e.replace(lk,"");for(;;){let i=l1.exec(n);if(!i)break;t.push(i[0])}n=n.replace(l1,"");let o=/@import[\s\S]*?url\([^)]*\)[\s\S]*?;/gi,r=new RegExp("((\\s*?(?:\\/\\*[\\s\\S]*?\\*\\/)?\\s*?@media[\\s\\S]*?){([\\s\\S]*?)}\\s*?})|(([\\s\\S]*?){([\\s\\S]*?)})","gi");for(;;){let i=o.exec(n);if(i)r.lastIndex=o.lastIndex;else if(i=r.exec(n),i)o.lastIndex=r.lastIndex;else break;t.push(i[0])}return t}var ck=/url\([^)]+\)\s*format\((["']?)([^"']+)\1\)/g,uk=/src:\s*(?:url\([^)]+\)\s*format\([^)]+\)[,;]\s*)+/g;function a1(e,t){let{font:n}=t,o=n?n?.preferredFormat:void 0;return o?e.replace(uk,r=>{for(;;){let[i,,s]=ck.exec(r)||[];if(!s)return"";if(s===o)return`src: ${i};`}}):e}function Om(e,t=[]){for(let n of Array.from(e))_b(n)?t.push(...Om(n.cssRules)):"cssRules"in n?Om(n.cssRules,t):t.push(n);return t}var dk=/\bx?link:?href\s*=\s*["'](?!data:)[^"']+["']/i;function _k(e){return dk.test(e.innerHTML)}async function mk(e,t){let n=await f1(e,t);if(Po(n.node)&&Wl(n.node)&&!_k(n.node))return n.node;let{ownerDocument:o,log:r,tasks:i,svgStyleElement:s,svgDefsElement:l,svgStyles:a,font:c,progress:d,autoDestruct:m,onCloneNode:f,onEmbedNode:k,onCreateForeignObjectSvg:w}=n;r.time("clone node");let C=await Hm(n.node,n,!0);if(s&&o){let E="";a.forEach((F,W)=>{E+=`${F.join(`,
`)} {
  ${W}
}
`}),s.appendChild(o.createTextNode(E))}r.timeEnd("clone node"),await f?.(C),c!==!1&&Po(C)&&(r.time("embed web font"),await sk(C,n),r.timeEnd("embed web font")),r.time("embed node"),k1(C,n);let S=i.length,g=0,x=async()=>{for(;;){let E=i.pop();if(!E)break;try{await E}catch(F){n.log.warn("Failed to run task",F)}d?.(++g,S)}};d?.(g,S),await Promise.all([...Array.from({length:4})].map(x)),r.timeEnd("embed node"),await k?.(C);let h=pk(C,n);return l&&h.insertBefore(l,h.children[0]),s&&h.insertBefore(s,h.children[0]),m&&Jb(n),await w?.(h),h}function pk(e,t){let{width:n,height:o}=t,r=Eb(n,o,e.ownerDocument),i=r.ownerDocument.createElementNS(r.namespaceURI,"foreignObject");return i.setAttributeNS(null,"x","0%"),i.setAttributeNS(null,"y","0%"),i.setAttributeNS(null,"width","100%"),i.setAttributeNS(null,"height","100%"),i.append(e),r.appendChild(i),r}async function S1(e,t){let n=await f1(e,t),o=await mk(n),r=Mb(o,n.isEnable("removeControlCharacter"));n.autoDestruct||(n.svgStyleElement=g1(n.ownerDocument),n.svgDefsElement=n.ownerDocument?.createElementNS(iu,"defs"),n.svgStyles.clear());let i=us(r,o.ownerDocument);return await $b(i,n)}var fk=1600,C1=2,M1="[data-agentation-portal], [data-agentation-root], [data-agentation-toolbar], [data-annotation-popup], agentation-toolbar";function gk(e){return e instanceof Element?e.matches(M1):!1}function hk(e){let t=document,n=Math.min(Math.max(e.x+e.width/2,0),window.innerWidth-1),o=Math.min(Math.max(e.y+e.height/2,0),window.innerHeight-1),r=t.elementsFromPoint(n,o);for(let i of r){if(!(i instanceof HTMLElement))continue;if(i===t.documentElement||i===t.body)break;if(i.closest(M1))continue;let s=i;for(;s&&s!==t.body&&s!==t.documentElement;){if(s.scrollTop!==0||s.scrollLeft!==0){s=null;break}s=s.parentElement}if(!s)continue;let l=i.getBoundingClientRect();if(l.left<=e.x&&l.top<=e.y&&l.right>=e.x+e.width&&l.bottom>=e.y+e.height)return i}return t.documentElement}function E1(){for(let e of[document.body,document.documentElement]){if(!e)continue;let t=getComputedStyle(e).backgroundColor;if(t&&t!=="transparent"&&!/rgba\(.*,\s*0\)$/.test(t))return t}return"#ffffff"}function yk(e,t,n,o,r,i){let s=Math.max(0,Math.min(i,o/2,r/2));e.beginPath(),e.moveTo(t+s,n),e.arcTo(t+o,n,t+o,n+r,s),e.arcTo(t+o,n+r,t,n+r,s),e.arcTo(t,n+r,t,n,s),e.arcTo(t,n,t+o,n,s),e.closePath(),e.stroke()}async function T1(e){if(typeof window>"u"||typeof document>"u")return null;let{rect:t}=e;if(t.width<C1&&t.height<C1)return null;let n=e.padding??24,o=e.minWidth??640,r=e.minHeight??400,i=-window.scrollX,s=-window.scrollY,l=i+Math.max(document.documentElement.scrollWidth,window.innerWidth),a=s+Math.max(document.documentElement.scrollHeight,window.innerHeight),c=(h,E,F,W,D)=>{let j=h-n,Z=h+E+n;if(Z-j<F){let q=(F-(Z-j))/2;j-=q,Z+=q}return j<W&&(Z+=W-j,j=W),Z>D&&(j-=Z-D,Z=D),j=Math.max(j,W),[j,Z-j]},[d,m]=c(t.x,t.width,o,i,l),[f,k]=c(t.y,t.height,r,s,a),w={x:d,y:f,width:m,height:k};if(w.width<1||w.height<1)return null;let C=hk(w),S=C.getBoundingClientRect(),g=Math.max(2,Math.min(window.devicePixelRatio||1,3)),x=Math.min(g,fk/Math.max(w.width,w.height));try{let h=await S1(C,{scale:g,backgroundColor:E1(),filter:q=>!gk(q),timeout:4e3});if(!h.width||!h.height||!S.width)return null;let E=h.width/Math.max(C.scrollWidth,S.width),F=Math.max(1,Math.round(w.width*x)),W=Math.max(1,Math.round(w.height*x)),D=document.createElement("canvas");D.width=F,D.height=W;let j=D.getContext("2d");if(!j)return null;j.fillStyle=E1(),j.fillRect(0,0,F,W),j.drawImage(h,(w.x-S.left)*E,(w.y-S.top)*E,w.width*E,w.height*E,0,0,F,W);let Z=e.highlights?.length?e.highlights:[t];j.strokeStyle=e.accentColor??"#3c82f7",j.lineWidth=Math.max(2,2*x);for(let q of Z)yk(j,(q.x-3-w.x)*x,(q.y-3-w.y)*x,(q.width+6)*x,(q.height+6)*x,6*x);return D.toDataURL("image/png")}catch(h){return console.warn("[Agentation] Screenshot capture failed:",h),null}}function Wm(e,t,n="markdown"){return n==="markdown"?t:[...new Set(e.map(o=>n==="source"?o.sourceFile:n==="classes"?o.cssClasses:o.attributes?.[n.attribute]).filter(o=>typeof o=="string"&&o.length>0))].join(`
`)}async function L1(e){if(typeof window>"u")return!1;try{if(navigator.clipboard?.writeText)return await navigator.clipboard.writeText(e),!0}catch{}return xk(e)}function xk(e){let t=document.createElement("textarea"),n=document.activeElement;for(;n?.shadowRoot?.activeElement;)n=n.shadowRoot.activeElement;let o=n instanceof HTMLInputElement||n instanceof HTMLTextAreaElement?n:null,r=o&&o.selectionStart!==null?{start:o.selectionStart,end:o.selectionEnd,direction:o.selectionDirection}:null,i=document.getSelection(),s=i?Array.from({length:i.rangeCount},(l,a)=>i.getRangeAt(a).cloneRange()):[];try{return t.value=e,t.setAttribute("readonly",""),t.style.cssText="position:fixed;left:-9999px;top:0;opacity:0;pointer-events:none;",document.body.appendChild(t),t.focus({preventScroll:!0}),t.select(),t.setSelectionRange(0,e.length),document.execCommand("copy")}catch{return!1}finally{if(t.remove(),n instanceof HTMLElement&&n.isConnected&&(n.focus({preventScroll:!0}),o&&r&&o.setSelectionRange(r.start,r.end,r.direction)),i){i.removeAllRanges();for(let l of s)i.addRange(l)}}}function R1(e){if(!e)return e;try{let t=new URL(e,"http://agentation.invalid");return t.pathname+t.search+t.hash}catch{return e}}function Um(e,t,n=[]){let o=new Map,r=new Map(n.map(l=>[l.id,l])),i=!1;async function s(l,a){if(i||a.running||a.timer)return;let c=a.desired,d=t.get(l);if(!c&&!d){o.delete(l),t.delete(l);return}let m=c?JSON.stringify(c):void 0;if(!(c&&d&&a.synced===m)){a.running=!0;try{if(!c)await e.remove(d),t.delete(l),a.synced=void 0;else if(d)await e.update(d,c),a.synced=m;else{t.set(l,"");let f=await e.create(c);t.set(l,f.id),a.synced=m}a.retries=0,a.running=!1,!i&&o.get(l)===a&&s(l,a)}catch(f){if(a.running=!1,!i&&o.get(l)===a&&(console.warn("[Agentation] Failed to sync layout feedback:",f),t.get(l)&&a.retries<3)){let k=500*2**a.retries++;a.timer=De(()=>{a.timer=void 0,s(l,a)},k)}}}}return{replace(l){let a=new Map(l.map(c=>[c.id,c]));for(let[c,d]of a){let m=o.get(c);if(!m){m={running:!1,retries:0},o.set(c,m);let f=[...r.values()].find(k=>k.kind===d.kind&&R1(k.url)===R1(d.url)&&(d.kind==="placement"?k.timestamp===d.timestamp&&k.element===d.element:k.element===d.element));f&&(t.set(c,f.id),r.delete(f.id))}JSON.stringify(m.desired)!==JSON.stringify(d)&&(m.retries=0,m.timer&&clearTimeout(m.timer),m.timer=void 0),m.desired=d}for(let[c,d]of o)a.has(c)||(d.desired=void 0),s(c,d)},forget(l){let a=o.get(l);a?.timer&&clearTimeout(a.timer),o.delete(l),t.delete(l)},dispose(){i=!0;for(let l of o.values())l.timer&&clearTimeout(l.timer)}}}function P1(e,t,n,o){let r=!1,i,s,l=1e3,a,c=k=>{!r&&(k?.status==="resolved"||k?.status==="dismissed")&&o(k)},d=async()=>{if(r||a||!n())return;let k=new AbortController;a=k;let w=De(()=>k.abort(),5e3);try{let C=await fetch(`${e}/sessions/${t}`,{signal:k.signal});if(!C.ok)return;let S=await C.json();!r&&!k.signal.aborted&&Array.isArray(S.annotations)&&S.annotations.forEach(c)}catch{}finally{clearTimeout(w),a===k&&(a=void 0)}},m=()=>{if(r)return;let k=new EventSource(`${e}/sessions/${t}/events`),w=()=>{l=1e3,d()},C=g=>{try{c(JSON.parse(g.data).payload)}catch{}},S=()=>{k.readyState!==EventSource.CLOSED||r||s!==void 0||(i?.(),s=De(()=>{s=void 0,m()},l),l=Math.min(l*2,1e4))};k.addEventListener("open",w),k.addEventListener("annotation.updated",C),k.addEventListener("error",S),i=()=>{k.removeEventListener("open",w),k.removeEventListener("annotation.updated",C),k.removeEventListener("error",S),k.close()}};m();let f=$c(()=>{d()},1e4);return()=>{r=!0,i?.(),s!==void 0&&clearTimeout(s),clearInterval(f),a?.abort()}}var Tn=ce(vt());var N1=`.styles-module__surface___7qnpJ {
  padding: 0;
  width: var(--preview-width, 200px);
  max-width: calc(100vw - 24px);
  overflow: auto;
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  border-radius: 12px;
  z-index: inherit;
  will-change: auto;
  transform: translateX(-50%);
  transition: left 200ms cubic-bezier(0.2, 0.8, 0.2, 1), top 200ms cubic-bezier(0.2, 0.8, 0.2, 1), transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1), width 200ms cubic-bezier(0.2, 0.8, 0.2, 1), opacity 100ms ease-out, visibility 0s 200ms;
}
.styles-module__surface___7qnpJ[data-positioning] {
  transition: none;
}
.styles-module__surface___7qnpJ[data-direct-entry] *, .styles-module__surface___7qnpJ[data-direct-entry] *::before, .styles-module__surface___7qnpJ[data-direct-entry] *::after {
  transition: none !important;
}
.styles-module__surface___7qnpJ[data-state=preview], .styles-module__surface___7qnpJ[data-state=edit] {
  opacity: 1;
  visibility: visible;
  transition-delay: 0s;
}
.styles-module__surface___7qnpJ[data-state=edit], .styles-module__surface___7qnpJ[data-annotation-popup][data-state=hidden] {
  width: 280px;
  border-radius: 16px;
}
.styles-module__surface___7qnpJ[data-state=edit] {
  pointer-events: auto;
}
.styles-module__surface___7qnpJ[data-state=preview] {
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3), 0 0 0 1px rgba(255, 255, 255, 0.08);
}
[data-agentation-theme=light] .styles-module__surface___7qnpJ[data-state=preview] {
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.12), 0 0 0 1px rgba(0, 0, 0, 0.06);
}

@media (prefers-reduced-motion: reduce) {
  .styles-module__surface___7qnpJ {
    transition: opacity 100ms ease-out, visibility 0s 100ms;
  }
}`,I1={surface:"styles-module__surface___7qnpJ"};var jm=ce(ze()),D1=(0,Tn.forwardRef)(function({annotation:t,editing:n,exiting:o,restorePreview:r,editorProps:i,lightMode:s,scrollY:l,onExited:a},c){let d=(0,Tn.useRef)({annotation:t,editorProps:i}),m=t??d.current.annotation,f=t?i:d.current.editorProps,k=(0,Tn.useRef)(null),w=(0,Tn.useRef)(null),C=(0,Tn.useRef)(),S=(0,Tn.useRef)(l),g=n&&!o?"edit":t&&(!n||r)?"preview":"hidden",x=g==="preview"||!n;return(0,Tn.useLayoutEffect)(()=>{t&&(d.current={annotation:t,editorProps:i})},[t,i]),(0,Tn.useLayoutEffect)(()=>{let h=k.current;if(!h||!m)return;let E=g==="edit"&&(C.current!==m.id||h.dataset.state==="hidden"&&getComputedStyle(h).opacity==="0");E&&(h.dataset.directEntry="true");let F=()=>{let j=m.x/100*window.innerWidth,Z=m.isFixed?m.y:m.y-l,q=g==="preview"||!n,O=parseFloat(h.style.getPropertyValue("--preview-width"))||200,Ae=Math.min(O,window.innerWidth-24),ue=Math.max(12,Math.min(window.innerWidth-Ae-12,j-Ae/2)),J=q?Ae:Math.min(280,window.innerWidth-24),Ce=Math.min(q?12:20,(window.innerWidth-J)/2),K=Z>window.innerHeight-(q?101:290);h.style.left=`${Math.max(Ce,Math.min(window.innerWidth-J-Ce,ue))}px`,h.style.right="auto",h.style.top=`${Math.max(12,Math.min(window.innerHeight-12,Z+(K?-21:21)))}px`,h.style.bottom="auto",h.style.transform=K?"translateY(-100%)":"translateY(0)",h.style.maxHeight=`${Math.max(100,K?Z-33:window.innerHeight-Z-33)}px`},W=C.current!==m.id||S.current!==l||h.dataset.state==="hidden";W&&(h.dataset.positioning="true"),F(),(W||E)&&h.getBoundingClientRect(),delete h.dataset.positioning,delete h.dataset.directEntry,h.dataset.state=g,h.inert=g!=="edit",C.current=m.id,S.current=l;let D=()=>{h.dataset.positioning="true",F(),h.getBoundingClientRect(),delete h.dataset.positioning};return window.addEventListener("resize",D),()=>window.removeEventListener("resize",D)},[m?.id,m?.x,m?.y,m?.isFixed,g,n,l,m?.comment]),(0,Tn.useLayoutEffect)(()=>{n&&!o&&w.current?.focus()},[n,o,m?.id]),rs(k,o,a),(0,Tn.useImperativeHandle)(c,()=>({shake(){k.current?.animate?.([{translate:"0px"},{translate:"-3px"},{translate:"3px"},{translate:"-2px"},{translate:"2px"},{translate:"0px"}],{duration:250}),w.current?.focus()}}),[]),!m||!f?null:(0,jm.jsx)("div",{ref:k,className:`${Re.popup} ${I1.surface} ${s?Re.light:""}`,"data-feedback-toolbar":!0,"data-annotation-card":!0,"data-annotation-popup":n?"":void 0,"data-state":"hidden","aria-hidden":g==="hidden",onClick:h=>h.stopPropagation(),onKeyDownCapture:h=>{h.key!=="Escape"||h.nativeEvent.isComposing||!n||(h.preventDefault(),h.stopPropagation(),f.onCancel())},children:(0,jm.jsx)(jc,{ref:w,...f,variant:"card",preview:x,resetOnPreview:!n,disabled:!n||o},m.id)})});var qn=ce(vt());var $1=`@keyframes styles-module__markerIn___x4G8D {
  0% {
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.3);
  }
  100% {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
}
@keyframes styles-module__markerOut___6VhQN {
  0% {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
  }
  100% {
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.3);
  }
}
@keyframes styles-module__renumberRoll___akV9B {
  0% {
    transform: translateX(-40%);
    opacity: 0;
  }
  100% {
    transform: translateX(0);
    opacity: 1;
  }
}
.styles-module__marker___9CKF7 {
  padding: 0;
  border: 0;
  box-sizing: border-box;
  font-family: inherit;
  line-height: 1;
  text-align: center;
  appearance: none;
  position: absolute;
  width: 22px;
  height: 22px;
  background: var(--agentation-color-blue);
  color: white;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.6875rem;
  font-weight: 600;
  transform: translate(-50%, -50%) scale(1);
  opacity: 1;
  cursor: pointer;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2), inset 0 0 0 1px rgba(0, 0, 0, 0.04);
  -webkit-user-select: none;
  user-select: none;
  will-change: transform, opacity;
  contain: layout style;
  z-index: 1;
}
.styles-module__marker___9CKF7 > * {
  pointer-events: none;
}
.styles-module__marker___9CKF7:focus-visible {
  outline: 2px solid var(--agentation-color-accent);
  outline-offset: 3px;
}
.styles-module__marker___9CKF7:hover, .styles-module__marker___9CKF7:focus-visible, .styles-module__marker___9CKF7.styles-module__previewVisible___imMag {
  z-index: 2;
}
.styles-module__marker___9CKF7:not(.styles-module__enter___8kI3q):not(.styles-module__exit___KBdR3):not(.styles-module__clearing___8rM7K):not(.styles-module__confirm___BtMvq) {
  transition: background-color 0.15s ease, transform 0.1s ease, z-index 0s 0.1s;
}
.styles-module__marker___9CKF7:not(.styles-module__enter___8kI3q):not(.styles-module__exit___KBdR3):not(.styles-module__clearing___8rM7K):not(.styles-module__confirm___BtMvq):hover, .styles-module__marker___9CKF7:not(.styles-module__enter___8kI3q):not(.styles-module__exit___KBdR3):not(.styles-module__clearing___8rM7K):not(.styles-module__confirm___BtMvq):focus-visible, .styles-module__marker___9CKF7:not(.styles-module__enter___8kI3q):not(.styles-module__exit___KBdR3):not(.styles-module__clearing___8rM7K):not(.styles-module__confirm___BtMvq).styles-module__previewVisible___imMag {
  transition-delay: 0s;
}
.styles-module__marker___9CKF7.styles-module__enter___8kI3q {
  animation: styles-module__markerIn___x4G8D 0.25s cubic-bezier(0.22, 1, 0.36, 1) both;
}
.styles-module__marker___9CKF7.styles-module__confirm___BtMvq {
  animation: styles-module__markerConfirm___RT4Sk 220ms ease-out both;
}
.styles-module__marker___9CKF7.styles-module__exit___KBdR3 {
  animation: styles-module__markerOut___6VhQN 0.2s ease-out both;
  pointer-events: none;
}
.styles-module__marker___9CKF7.styles-module__clearing___8rM7K {
  animation: styles-module__markerOut___6VhQN 0.15s ease-out both;
  pointer-events: none;
}
.styles-module__marker___9CKF7:not(.styles-module__enter___8kI3q):not(.styles-module__exit___KBdR3):not(.styles-module__clearing___8rM7K):not(.styles-module__confirm___BtMvq):hover {
  transform: translate(-50%, -50%) scale(1.1);
}
.styles-module__marker___9CKF7.styles-module__pending___BiY-U {
  background-color: var(--agentation-color-blue);
  cursor: default;
}
.styles-module__marker___9CKF7.styles-module__pending___BiY-U.styles-module__exit___KBdR3 {
  animation-duration: 150ms;
}
.styles-module__marker___9CKF7.styles-module__multiSelect___CPfTC {
  background-color: var(--agentation-color-green);
  width: 26px;
  height: 26px;
  border-radius: 6px;
  font-size: 0.75rem;
}
.styles-module__marker___9CKF7.styles-module__multiSelect___CPfTC.styles-module__pending___BiY-U {
  background-color: var(--agentation-color-green);
}
.styles-module__marker___9CKF7.styles-module__hovered___-mg2N {
  background-color: var(--agentation-color-red);
}

.styles-module__renumber___16lvD {
  display: block;
  animation: styles-module__renumberRoll___akV9B 0.2s ease-out;
}

@keyframes styles-module__markerConfirm___RT4Sk {
  0% {
    transform: translate(-50%, -50%) scale(1);
  }
  25% {
    transform: translate(-50%, -50%) scale(0.94);
  }
  65% {
    transform: translate(-50%, -50%) scale(1.06);
  }
  100% {
    transform: translate(-50%, -50%) scale(1);
  }
}
.styles-module__number___1JFu9 {
  display: block;
}

.styles-module__numberGlyph___qchdk {
  display: block;
  opacity: 1;
  transform: translateY(0);
  filter: blur(0);
  transition: opacity 140ms ease-out, transform 180ms cubic-bezier(0.22, 1, 0.36, 1), filter 140ms ease-out;
}

.styles-module__actionGlyph___AFRt0 {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  opacity: 0;
  transform: translateY(2px) scale(0.8) rotate(-12deg);
  filter: blur(1px);
  transition: opacity 120ms ease-out, transform 160ms cubic-bezier(0.22, 1, 0.36, 1), filter 120ms ease-out;
}

.styles-module__actionVisible___Kb--l .styles-module__numberGlyph___qchdk {
  opacity: 0;
  transform: translateY(-2px) scale(0.8);
  filter: blur(1px);
}
.styles-module__actionVisible___Kb--l .styles-module__actionGlyph___AFRt0 {
  opacity: 1;
  transform: translateY(0) scale(1) rotate(0);
  filter: blur(0);
}

.styles-module__plus___xslMP {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  opacity: 0;
  transform: rotate(-90deg) scale(0.6);
  filter: blur(1px);
  transition: opacity 100ms ease-out, transform 140ms ease-out, filter 100ms ease-out;
  pointer-events: none;
}

.styles-module__pending___BiY-U .styles-module__numberGlyph___qchdk {
  opacity: 0;
  transform: translateY(5px);
  filter: blur(2px);
}
.styles-module__pending___BiY-U .styles-module__plus___xslMP {
  opacity: 1;
  transform: rotate(0) scale(1);
  filter: blur(0);
}

@media (prefers-reduced-motion: reduce) {
  .styles-module__marker___9CKF7.styles-module__enter___8kI3q, .styles-module__marker___9CKF7.styles-module__exit___KBdR3, .styles-module__marker___9CKF7.styles-module__clearing___8rM7K, .styles-module__marker___9CKF7.styles-module__confirm___BtMvq {
    animation-duration: 1ms;
    animation-delay: 0ms !important;
  }
  .styles-module__numberGlyph___qchdk, .styles-module__actionGlyph___AFRt0, .styles-module__plus___xslMP {
    transition: opacity 100ms ease-out;
    transform: none;
    filter: none;
  }
  .styles-module__pending___BiY-U .styles-module__numberGlyph___qchdk, .styles-module__pending___BiY-U .styles-module__plus___xslMP,
  .styles-module__actionVisible___Kb--l .styles-module__numberGlyph___qchdk, .styles-module__actionVisible___Kb--l .styles-module__actionGlyph___AFRt0 {
    transform: none;
    filter: none;
  }
}`,qt={marker:"styles-module__marker___9CKF7",previewVisible:"styles-module__previewVisible___imMag",enter:"styles-module__enter___8kI3q",exit:"styles-module__exit___KBdR3",clearing:"styles-module__clearing___8rM7K",confirm:"styles-module__confirm___BtMvq",markerIn:"styles-module__markerIn___x4G8D",markerConfirm:"styles-module__markerConfirm___RT4Sk",markerOut:"styles-module__markerOut___6VhQN",pending:"styles-module__pending___BiY-U",multiSelect:"styles-module__multiSelect___CPfTC",hovered:"styles-module__hovered___-mg2N",renumber:"styles-module__renumber___16lvD",renumberRoll:"styles-module__renumberRoll___akV9B",number:"styles-module__number___1JFu9",numberGlyph:"styles-module__numberGlyph___qchdk",actionGlyph:"styles-module__actionGlyph___AFRt0",actionVisible:"styles-module__actionVisible___Kb--l",plus:"styles-module__plus___xslMP"};var No=ce(ze()),Ym=(0,qn.memo)(function({annotation:t,pending:n=!1,globalIndex:o,layerIndex:r,layerSize:i,isExiting:s,isClearing:l,isAnimated:a,isNew:c,isHovered:d,isRemoving:m,onRemoveComplete:f,isEditingAny:k,renumberFrom:w,markerClickBehavior:C,onHoverEnter:S,onEnterComplete:g,onHoverLeave:x,onClick:h,onContextMenu:E}){let[F,W]=(0,qn.useState)(a),D=(0,qn.useRef)(n),[j,Z]=(0,qn.useState)(!1),q=D.current&&!n&&F&&!j;(0,qn.useLayoutEffect)(()=>{s&&W(!1)},[s]);let O=(0,qn.useRef)(null),Ae=(0,qn.useRef)({action:!1,delete:!1}),ue=d&&!k,J=ue&&C==="delete";(0,qn.useLayoutEffect)(()=>{m||(Ae.current={action:ue,delete:J})},[m,ue,J]);let Ce=m?Ae.current.action:ue,K=m?Ae.current.delete:J;rs(O,m,()=>f(t.id));let Ge=t.isMultiSelect,Le=Ge?"var(--agentation-color-green)":"var(--agentation-color-accent)",Ye=l?qt.clearing:s||m?qt.exit:q?qt.confirm:!a&&!F?qt.enter:"",xe=l?`${Math.min(r*20,120)}ms`:m||n||q?"0ms":s?`${(i-1-r)*20}ms`:`${c?0:r*20}ms`;return(0,No.jsxs)("button",{ref:O,type:"button","aria-label":n?"Pending annotation":`${C==="delete"?"Delete":"Edit"} annotation ${o+1}: ${t.element}`,disabled:n||s||m||l,tabIndex:n||k?-1:0,className:`${qt.marker} ${n?qt.pending:""} ${Ge?qt.multiSelect:""} ${Ye} ${!n&&Ce?qt.actionVisible:""} ${K?qt.hovered:""} ${d&&!k&&!m?qt.previewVisible:""}`,"data-annotation-marker":n?void 0:"","data-annotation-pending":n?"":void 0,style:{left:`${t.x}%`,top:t.y,backgroundColor:K?void 0:Le,animationDelay:xe},onAnimationEnd:gt=>{gt.target===gt.currentTarget&&(Ye===qt.enter||Ye===qt.confirm)&&(W(!0),n||Z(!0),n||g(t.id))},onMouseOver:()=>{n||S(t)},onMouseOut:gt=>{let Ft=gt.relatedTarget;(!(Ft instanceof Node)||!gt.currentTarget.contains(Ft))&&x(t.id)},onFocus:gt=>{!n&&gt.currentTarget.matches(":focus-visible")&&S(t)},onBlur:()=>x(t.id),onClick:gt=>{gt.stopPropagation(),!n&&!s&&!m&&h(t,gt.currentTarget)},onContextMenu:E?gt=>{C==="delete"&&(gt.preventDefault(),gt.stopPropagation(),!n&&!s&&!m&&E(t,gt.currentTarget))}:void 0,children:[(0,No.jsx)("span",{className:`${qt.number} ${w!==null&&o>=w?qt.renumber:""}`,"aria-hidden":"true",children:(0,No.jsx)("span",{className:qt.numberGlyph,children:o+1})},o),(0,No.jsx)("span",{className:qt.actionGlyph,"aria-hidden":"true",children:C==="delete"?(0,No.jsx)(oy,{size:Ge?18:16}):(0,No.jsx)(sy,{size:16})}),D.current&&(0,No.jsx)("span",{className:qt.plus,"aria-hidden":"true",children:(0,No.jsx)(G0,{size:12})})]})});var Sr=ce(vt());var A1=`.styles-module__switchContainer___Ka-AB {
  display: flex;
  align-items: center;
  position: relative;
  padding: 2px;
  width: 24px;
  height: 16px;
  border-radius: 8px;
  background-color: #cdcdcd;
  transition: background-color 0.15s, opacity 0.15s;
}
[data-agentation-theme=dark] .styles-module__switchContainer___Ka-AB {
  background-color: #484848;
}
.styles-module__switchContainer___Ka-AB:has(.styles-module__switchInput___kYDSD:checked) {
  background-color: var(--agentation-color-blue);
}
.styles-module__switchContainer___Ka-AB:has(.styles-module__switchInput___kYDSD:disabled) {
  opacity: 0.3;
}

.styles-module__switchInput___kYDSD {
  position: absolute;
  z-index: 1;
  inset: 0;
  border-radius: inherit;
  opacity: 0;
  cursor: pointer;
}
.styles-module__switchInput___kYDSD:disabled {
  cursor: not-allowed;
}

.styles-module__switchThumb___4sCPH {
  border-radius: 50%;
  width: 12px;
  height: 12px;
  background-color: #fff;
  transition: transform 0.15s;
}
.styles-module__switchContainer___Ka-AB[data-checked] .styles-module__switchThumb___4sCPH {
  transform: translateX(8px);
}`,su={switchContainer:"styles-module__switchContainer___Ka-AB",switchInput:"styles-module__switchInput___kYDSD",switchThumb:"styles-module__switchThumb___4sCPH"};var Ul=ce(ze()),lu=({className:e="",checked:t,onChange:n,...o})=>(0,Ul.jsxs)("div",{className:`${su.switchContainer} ${e}`,"data-checked":t?"":void 0,children:[(0,Ul.jsx)("input",{className:su.switchInput,checked:t,onChange:n,type:"checkbox",...o}),(0,Ul.jsx)("div",{className:su.switchThumb})]});var z1=ce(vt());var F1=`.styles-module__checkboxContainer___joqZk {
  display: flex;
  justify-content: center;
  align-items: center;
  position: relative;
  border: 1px solid rgba(26, 26, 26, 0.2);
  border-radius: 4px;
  width: 14px;
  height: 14px;
  background-color: #fff;
  transition: background-color 0.2s ease;
}
[data-agentation-theme=dark] .styles-module__checkboxContainer___joqZk {
  border-color: rgba(255, 255, 255, 0.2);
  background-color: #252525;
}
.styles-module__checkboxContainer___joqZk:has(.styles-module__checkboxInput___ECzzO:checked) {
  background-color: #1a1a1a;
}
[data-agentation-theme=dark] .styles-module__checkboxContainer___joqZk:has(.styles-module__checkboxInput___ECzzO:checked) {
  background-color: #fff;
}

.styles-module__checkboxInput___ECzzO {
  position: absolute;
  z-index: 1;
  inset: -1px;
  border-radius: inherit;
  opacity: 0;
  cursor: pointer;
}

.styles-module__checkboxCheck___fUXpr {
  color: #fafafa;
}
[data-agentation-theme=dark] .styles-module__checkboxCheck___fUXpr {
  color: #1a1a1a;
}

.styles-module__checkboxCheckPath___cDyh8 {
  stroke-dasharray: 9.29px;
  stroke-dashoffset: 9.29px;
  color: #fafafa;
  transition: stroke-dashoffset 0.1s ease;
}
[data-agentation-theme=dark] .styles-module__checkboxCheckPath___cDyh8 {
  color: #1a1a1a;
}
.styles-module__checkboxContainer___joqZk[data-checked] .styles-module__checkboxCheckPath___cDyh8 {
  transition-duration: 0.2s;
  stroke-dashoffset: 0;
}`,jl={checkboxContainer:"styles-module__checkboxContainer___joqZk",checkboxInput:"styles-module__checkboxInput___ECzzO",checkboxCheck:"styles-module__checkboxCheck___fUXpr",checkboxCheckPath:"styles-module__checkboxCheckPath___cDyh8"};var _s=ce(ze()),O1=({className:e="",checked:t,onChange:n,...o})=>(0,_s.jsxs)("div",{className:`${jl.checkboxContainer} ${e}`,"data-checked":t?"":void 0,children:[(0,_s.jsx)("input",{className:jl.checkboxInput,type:"checkbox",checked:t,onChange:n,...o}),(0,_s.jsx)("svg",{className:jl.checkboxCheck,width:"14",height:"14",viewBox:"0 0 14 14",fill:"none",children:(0,_s.jsx)("path",{className:jl.checkboxCheckPath,d:"M3.94 7L6.13 9.19L10.5 4.81",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})})]});var B1=`.styles-module__container___w8eAF {
  display: flex;
  align-items: center;
  height: 24px;
}

.styles-module__label___J5mxE {
  padding-inline: 8px 2px;
  line-height: 20px;
  font-size: 13px;
  letter-spacing: -0.15px;
  color: rgba(26, 26, 26, 0.5);
  -webkit-user-select: none;
  user-select: none;
  cursor: pointer;
}
[data-agentation-theme=dark] .styles-module__label___J5mxE {
  color: rgba(255, 255, 255, 0.5);
}`,Vm={container:"styles-module__container___w8eAF",label:"styles-module__label___J5mxE"};var ms=ce(ze()),Xm=({className:e="",label:t,tooltip:n,checked:o,onChange:r,...i})=>{let s=(0,z1.useId)();return(0,ms.jsxs)("div",{className:`${Vm.container} ${e}`,...i,children:[(0,ms.jsx)(O1,{id:s,onChange:r,checked:o}),(0,ms.jsx)("label",{className:Vm.label,htmlFor:s,children:t}),n&&(0,ms.jsx)(Ro,{content:n})]})};var H1=`@keyframes styles-module__cycleTextIn___VBNTi {
  0% {
    opacity: 0;
    transform: translateY(-6px);
  }
  100% {
    opacity: 1;
    transform: translateY(0);
  }
}
@keyframes styles-module__scaleIn___QpQ8E {
  from {
    opacity: 0;
    transform: scale(0.85);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
@keyframes styles-module__mcpPulse___5Q3Jj {
  0% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-green) 50%, transparent);
  }
  70% {
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--agentation-color-green) 0%, transparent);
  }
  100% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-green) 0%, transparent);
  }
}
@keyframes styles-module__mcpPulseError___VHxhx {
  0% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-red) 50%, transparent);
  }
  70% {
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--agentation-color-red) 0%, transparent);
  }
  100% {
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--agentation-color-red) 0%, transparent);
  }
}
@keyframes styles-module__themeIconIn___qUWMV {
  0% {
    opacity: 0;
    transform: scale(0.8) rotate(-30deg);
  }
  100% {
    opacity: 1;
    transform: scale(1) rotate(0deg);
  }
}
.styles-module__settingsPanel___qNkn- :where(button, a, input, select, textarea):focus-visible {
  outline: 2px solid var(--agentation-color-accent);
  outline-offset: 2px;
}
.styles-module__settingsPanel___qNkn- {
  position: absolute;
  right: 5px;
  bottom: calc(100% + 0.5rem);
  z-index: 1;
  overflow: hidden;
  background: #1c1c1c;
  border-radius: 16px;
  padding: 12px 0;
  width: 253px;
  max-width: calc(100vw - 20px);
  cursor: default;
  opacity: 1;
  box-shadow: 0 1px 8px rgba(0, 0, 0, 0.25), 0 0 0 1px rgba(0, 0, 0, 0.04);
  transition: background-color 0.25s ease, box-shadow 0.25s ease;
}
.styles-module__settingsPanel___qNkn-::before, .styles-module__settingsPanel___qNkn-::after {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  width: 16px;
  z-index: 2;
  pointer-events: none;
}
.styles-module__settingsPanel___qNkn-::before {
  left: 0;
  background: linear-gradient(to right, #1c1c1c 0%, transparent 100%);
}
.styles-module__settingsPanel___qNkn-::after {
  right: 0;
  background: linear-gradient(to left, #1c1c1c 0%, transparent 100%);
}
.styles-module__settingsPanel___qNkn- .styles-module__settingsHeader___Fn1DP,
.styles-module__settingsPanel___qNkn- .styles-module__settingsBrand___OoKlM,
.styles-module__settingsPanel___qNkn- .styles-module__settingsVersion___rXmL9,
.styles-module__settingsPanel___qNkn- .styles-module__settingsSection___n5V-4,
.styles-module__settingsPanel___qNkn- .styles-module__settingsLabel___VCVOQ,
.styles-module__settingsPanel___qNkn- .styles-module__cycleButton___XMBx3,
.styles-module__settingsPanel___qNkn- .styles-module__cycleDot___zgSXY,
.styles-module__settingsPanel___qNkn- .styles-module__dropdownButton___mKHe8,
.styles-module__settingsPanel___qNkn- .styles-module__sliderLabel___6K5v1,
.styles-module__settingsPanel___qNkn- .styles-module__slider___v5z-c,
.styles-module__settingsPanel___qNkn- .styles-module__themeToggle___3imlT {
  transition: background-color 0.25s ease, color 0.25s ease, border-color 0.25s ease;
}
.styles-module__settingsPanel___qNkn- {
  opacity: 0;
  transform: translateY(var(--panel-offset-y, 4px)) scale(0.98);
  transform-origin: var(--panel-origin, bottom right);
  filter: blur(2px);
  pointer-events: none;
  visibility: hidden;
  transition: opacity 120ms cubic-bezier(0.25, 0.46, 0.45, 0.94), transform 120ms cubic-bezier(0.25, 0.46, 0.45, 0.94), filter 120ms cubic-bezier(0.25, 0.46, 0.45, 0.94);
}
.styles-module__settingsPanel___qNkn-[data-panel-present=true] {
  visibility: visible;
}
.styles-module__settingsPanel___qNkn-[data-panel-open=true] {
  opacity: 1;
  transform: translateY(0) scale(1);
  filter: blur(0);
  pointer-events: auto;
  transition-duration: 160ms;
}
@media (prefers-reduced-motion: reduce) {
  .styles-module__settingsPanel___qNkn- {
    transition: none;
    transform: none;
    filter: none;
  }
}
.styles-module__settingsPanel___qNkn-.styles-module__below___Vpv-k {
  --panel-offset-y: -4px;
  --panel-origin: top right;
}
[data-agentation-theme=dark] .styles-module__settingsPanel___qNkn- {
  background: #1a1a1a;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3), 0 0 0 1px rgba(255, 255, 255, 0.08);
}
[data-agentation-theme=dark] .styles-module__settingsPanel___qNkn- .styles-module__settingsLabel___VCVOQ {
  color: rgba(255, 255, 255, 0.6);
}
[data-agentation-theme=dark] .styles-module__settingsPanel___qNkn- .styles-module__settingsOption___JoyH- {
  color: rgba(255, 255, 255, 0.85);
}
[data-agentation-theme=dark] .styles-module__settingsPanel___qNkn- .styles-module__settingsOption___JoyH-:hover {
  background: rgba(255, 255, 255, 0.1);
}
[data-agentation-theme=dark] .styles-module__settingsPanel___qNkn- .styles-module__settingsOption___JoyH-.styles-module__selected___k1-Vq {
  background: rgba(255, 255, 255, 0.15);
  color: #fff;
}

.styles-module__settingsPanelContainer___5it-H {
  overflow: visible;
  position: relative;
  display: flex;
  padding: 0 16px;
}

.styles-module__settingsPage___BMn-3 {
  min-width: 100%;
  flex-basis: 0;
  flex-shrink: 0;
  transition: transform 0.2s ease, opacity 0.2s ease;
  transition-delay: 0s;
  opacity: 1;
}

.styles-module__settingsPage___BMn-3.styles-module__slideLeft___qUvW4 {
  transform: translateX(-24px);
  opacity: 0;
  pointer-events: none;
}

.styles-module__automationsPage___N7By0 {
  position: absolute;
  top: 0;
  left: 24px;
  width: 100%;
  height: 100%;
  padding: 0 16px 4px;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  transition: transform 0.2s ease, opacity 0.2s ease;
  opacity: 0;
  pointer-events: none;
}

.styles-module__automationsPage___N7By0.styles-module__slideIn___uXDSu {
  transform: translateX(-24px);
  opacity: 1;
  pointer-events: auto;
}

.styles-module__settingsHeader___Fn1DP {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 24px;
}

.styles-module__settingsBrand___OoKlM {
  display: flex;
  align-items: center;
  font-size: 0.8125rem;
  font-weight: 500;
  letter-spacing: -0.0094em;
  color: #bbb;
  text-decoration: none;
}

.styles-module__settingsVersion___rXmL9 {
  font-size: 11px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.4);
  margin-left: 6px;
  letter-spacing: -0.0094em;
}

.styles-module__themeToggle___3imlT {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  margin-left: auto;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: rgba(255, 255, 255, 0.4);
  transition: background-color 0.15s ease, color 0.15s ease;
  cursor: pointer;
}
.styles-module__themeToggle___3imlT:hover {
  background: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.8);
}
[data-agentation-theme=light] .styles-module__themeToggle___3imlT {
  color: rgba(0, 0, 0, 0.4);
}
[data-agentation-theme=light] .styles-module__themeToggle___3imlT:hover {
  background: rgba(0, 0, 0, 0.06);
  color: rgba(0, 0, 0, 0.7);
}

.styles-module__themeIconWrapper___pyaYa {
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  width: 20px;
  height: 20px;
}

.styles-module__themeIcon___w7lAm {
  display: flex;
  align-items: center;
  justify-content: center;
  animation: styles-module__themeIconIn___qUWMV 0.35s cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
}

.styles-module__settingsSectionGrow___eZTRw {
  flex: 1;
  display: flex;
  flex-direction: column;
}

.styles-module__settingsRow___y-tDE {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 24px;
}
.styles-module__settingsRow___y-tDE.styles-module__settingsRowMarginTop___uLpGb {
  margin-top: 8px;
}

.styles-module__settingsRowDisabled___ydl3Q .styles-module__settingsLabel___VCVOQ {
  color: rgba(255, 255, 255, 0.2);
}
[data-agentation-theme=light] .styles-module__settingsRowDisabled___ydl3Q .styles-module__settingsLabel___VCVOQ {
  color: rgba(0, 0, 0, 0.2);
}

.styles-module__settingsLabel___VCVOQ {
  display: flex;
  align-items: center;
  column-gap: 2px;
  line-height: 20px;
  font-size: 13px;
  font-weight: 400;
  letter-spacing: -0.15px;
  color: rgba(255, 255, 255, 0.5);
}
[data-agentation-theme=light] .styles-module__settingsLabel___VCVOQ {
  color: rgba(0, 0, 0, 0.5);
}

.styles-module__cycleButton___XMBx3 {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0;
  border: none;
  background: transparent;
  font-size: 0.8125rem;
  font-weight: 500;
  color: #fff;
  cursor: pointer;
  letter-spacing: -0.0094em;
}
[data-agentation-theme=light] .styles-module__cycleButton___XMBx3 {
  color: rgba(0, 0, 0, 0.85);
}
.styles-module__cycleButton___XMBx3:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.styles-module__cycleButtonText___mbbnD {
  display: inline-block;
  animation: styles-module__cycleTextIn___VBNTi 0.2s ease-out;
}

.styles-module__cycleDots___ehp6i {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.styles-module__cycleDot___zgSXY {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.3);
  transform: scale(0.667);
  transition: background-color 0.25s ease-out, transform 0.25s ease-out;
}
.styles-module__cycleDot___zgSXY.styles-module__active___dpAhM {
  background: #fff;
  transform: scale(1);
}
[data-agentation-theme=light] .styles-module__cycleDot___zgSXY {
  background: rgba(0, 0, 0, 0.2);
}
[data-agentation-theme=light] .styles-module__cycleDot___zgSXY.styles-module__active___dpAhM {
  background: rgba(0, 0, 0, 0.7);
}

.styles-module__colorOptions___pbxZx {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 6px;
  height: 26px;
}

.styles-module__colorOption___Co955 {
  padding: 0;
  position: relative;
  border-radius: 50%;
  width: 20px;
  height: 20px;
  background-color: #fff;
  cursor: pointer;
}
[data-agentation-theme=dark] .styles-module__colorOption___Co955 {
  background-color: #1a1a1a;
}
.styles-module__colorOption___Co955::before, .styles-module__colorOption___Co955::after {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: 50%;
  background-color: var(--swatch);
  transition: opacity 0.2s, transform 0.2s;
}
@supports (color: color(display-p3 0 0 0)) {
  .styles-module__colorOption___Co955::before, .styles-module__colorOption___Co955::after {
    --color: var(--swatch-p3);
  }
}
.styles-module__colorOption___Co955::after {
  z-index: -1;
  transform: scale(1.2);
  opacity: 0;
}
.styles-module__colorOption___Co955.styles-module__selected___k1-Vq::before {
  transform: scale(0.8);
}
.styles-module__colorOption___Co955.styles-module__selected___k1-Vq::after {
  opacity: 1;
}

.styles-module__settingsNavLink___uYIwM {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  height: 24px;
  padding: 0;
  border: none;
  background: transparent;
  font-family: inherit;
  line-height: 20px;
  font-size: 13px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.5);
  transition: color 0.15s ease;
  cursor: pointer;
}
.styles-module__settingsNavLink___uYIwM:hover {
  color: rgba(255, 255, 255, 0.9);
}
.styles-module__settingsNavLink___uYIwM svg {
  color: rgba(255, 255, 255, 0.4);
  transition: color 0.15s ease;
}
.styles-module__settingsNavLink___uYIwM:hover svg {
  color: #fff;
}
[data-agentation-theme=light] .styles-module__settingsNavLink___uYIwM {
  color: rgba(0, 0, 0, 0.5);
}
[data-agentation-theme=light] .styles-module__settingsNavLink___uYIwM:hover {
  color: rgba(0, 0, 0, 0.8);
}
[data-agentation-theme=light] .styles-module__settingsNavLink___uYIwM svg {
  color: rgba(0, 0, 0, 0.25);
}
[data-agentation-theme=light] .styles-module__settingsNavLink___uYIwM:hover svg {
  color: rgba(0, 0, 0, 0.8);
}

.styles-module__settingsNavLinkRight___XBUzC {
  display: flex;
  align-items: center;
  gap: 6px;
}

.styles-module__settingsBackButton___fflll {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  border: none;
  background: transparent;
  font-family: inherit;
  line-height: 20px;
  font-size: 13px;
  font-weight: 500;
  letter-spacing: -0.15px;
  color: #fff;
  cursor: pointer;
  transition: transform 0.12s cubic-bezier(0.32, 0.72, 0, 1);
}
.styles-module__settingsBackButton___fflll svg {
  opacity: 0.4;
  flex-shrink: 0;
  transition: opacity 0.15s ease, transform 0.18s cubic-bezier(0.32, 0.72, 0, 1);
}
.styles-module__settingsBackButton___fflll:hover svg {
  opacity: 1;
}
[data-agentation-theme=light] .styles-module__settingsBackButton___fflll {
  color: rgba(0, 0, 0, 0.85);
  border-bottom-color: rgba(0, 0, 0, 0.08);
}

.styles-module__automationHeader___Avra9 {
  display: flex;
  align-items: center;
  gap: 0.125rem;
  font-size: 0.8125rem;
  font-weight: 400;
  color: #fff;
}
[data-agentation-theme=light] .styles-module__automationHeader___Avra9 {
  color: rgba(0, 0, 0, 0.85);
}

.styles-module__automationDescription___vFTmJ {
  font-size: 0.6875rem;
  font-weight: 300;
  color: rgba(255, 255, 255, 0.5);
  margin-top: 2px;
  line-height: 14px;
}
[data-agentation-theme=light] .styles-module__automationDescription___vFTmJ {
  color: rgba(0, 0, 0, 0.5);
}

.styles-module__learnMoreLink___cG7OI {
  color: rgba(255, 255, 255, 0.8);
  text-decoration-line: underline;
  text-decoration-style: dotted;
  text-decoration-color: rgba(255, 255, 255, 0.2);
  text-underline-offset: 2px;
  transition: color 0.15s ease;
}
.styles-module__learnMoreLink___cG7OI:hover {
  color: #fff;
}
[data-agentation-theme=light] .styles-module__learnMoreLink___cG7OI {
  color: rgba(0, 0, 0, 0.6);
  text-decoration-color: rgba(0, 0, 0, 0.2);
}
[data-agentation-theme=light] .styles-module__learnMoreLink___cG7OI:hover {
  color: rgba(0, 0, 0, 0.85);
}

.styles-module__autoSendContainer___VpkXk {
  display: flex;
  align-items: center;
}

.styles-module__autoSendLabel___ngNdC {
  padding-inline-end: 8px;
  font-size: 11px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.4);
  transition: color 0.15s, opacity 0.15s;
  cursor: pointer;
}
.styles-module__autoSendLabel___ngNdC.styles-module__active___dpAhM {
  color: #66b8ff;
  color: color(display-p3 0.4 0.72 1);
}
[data-agentation-theme=light] .styles-module__autoSendLabel___ngNdC {
  color: rgba(0, 0, 0, 0.4);
}
[data-agentation-theme=light] .styles-module__autoSendLabel___ngNdC.styles-module__active___dpAhM {
  color: var(--agentation-color-blue);
}
.styles-module__autoSendLabel___ngNdC.styles-module__disabled___9AZYS {
  opacity: 0.3;
  cursor: not-allowed;
}

.styles-module__mcpStatusDot___8AMxP {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.styles-module__mcpStatusDot___8AMxP.styles-module__connecting___QEO1r {
  background-color: var(--agentation-color-yellow);
  animation: styles-module__mcpPulse___5Q3Jj 1.5s infinite;
}
.styles-module__mcpStatusDot___8AMxP.styles-module__connected___WyFkx {
  background-color: var(--agentation-color-green);
  animation: styles-module__mcpPulse___5Q3Jj 2.5s ease-in-out infinite;
}
.styles-module__mcpStatusDot___8AMxP.styles-module__disconnected___mvmvQ {
  background-color: var(--agentation-color-red);
  animation: styles-module__mcpPulseError___VHxhx 2s infinite;
}

.styles-module__mcpNavIndicator___auBHI {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.styles-module__mcpNavIndicator___auBHI.styles-module__connected___WyFkx {
  background-color: var(--agentation-color-green);
  animation: styles-module__mcpPulse___5Q3Jj 2.5s ease-in-out infinite;
}
.styles-module__mcpNavIndicator___auBHI.styles-module__connecting___QEO1r {
  background-color: var(--agentation-color-yellow);
  animation: styles-module__mcpPulse___5Q3Jj 1.5s ease-in-out infinite;
}

.styles-module__webhookUrlInput___WDDDC {
  display: block;
  width: 100%;
  flex: 1;
  min-height: 60px;
  box-sizing: border-box;
  margin-top: 11px;
  padding: 8px 10px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.03);
  font-family: inherit;
  font-size: 0.75rem;
  font-weight: 400;
  color: #fff;
  outline: none;
  resize: none;
  user-select: text;
  transition: border-color 0.15s ease, background-color 0.15s ease, box-shadow 0.15s ease;
}
.styles-module__webhookUrlInput___WDDDC::placeholder {
  color: rgba(255, 255, 255, 0.3);
}
.styles-module__webhookUrlInput___WDDDC:focus {
  border-color: rgba(255, 255, 255, 0.3);
  background: rgba(255, 255, 255, 0.08);
}
[data-agentation-theme=light] .styles-module__webhookUrlInput___WDDDC {
  border-color: rgba(0, 0, 0, 0.1);
  background: rgba(0, 0, 0, 0.03);
  color: rgba(0, 0, 0, 0.85);
}
[data-agentation-theme=light] .styles-module__webhookUrlInput___WDDDC::placeholder {
  color: rgba(0, 0, 0, 0.3);
}
[data-agentation-theme=light] .styles-module__webhookUrlInput___WDDDC:focus {
  border-color: rgba(0, 0, 0, 0.25);
  background: rgba(0, 0, 0, 0.05);
}

[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- {
  background: #fff;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08), 0 4px 16px rgba(0, 0, 0, 0.06), 0 0 0 1px rgba(0, 0, 0, 0.04);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn-::before {
  background: linear-gradient(to right, #fff 0%, transparent 100%);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn-::after {
  background: linear-gradient(to left, #fff 0%, transparent 100%);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__settingsHeader___Fn1DP {
  border-bottom-color: rgba(0, 0, 0, 0.08);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__settingsBrand___OoKlM {
  color: #333;
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__settingsVersion___rXmL9 {
  color: rgba(0, 0, 0, 0.4);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__settingsSection___n5V-4 {
  border-top-color: rgba(0, 0, 0, 0.08);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__settingsLabel___VCVOQ {
  color: rgba(0, 0, 0, 0.5);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__cycleButton___XMBx3 {
  color: rgba(0, 0, 0, 0.85);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__cycleDot___zgSXY {
  background: rgba(0, 0, 0, 0.2);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__cycleDot___zgSXY.styles-module__active___dpAhM {
  background: rgba(0, 0, 0, 0.7);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__dropdownButton___mKHe8 {
  color: rgba(0, 0, 0, 0.85);
}
[data-agentation-theme=light] .styles-module__settingsPanel___qNkn- .styles-module__dropdownButton___mKHe8:hover {
  background: rgba(0, 0, 0, 0.05);
}

.styles-module__checkboxField___ZrSqv:not(:first-child) {
  margin-top: 8px;
}

.styles-module__divider___h6Yux {
  margin-block: 8px;
  width: 100%;
  height: 1px;
  background-color: rgba(26, 26, 26, 0.07);
}
[data-agentation-theme=dark] .styles-module__divider___h6Yux {
  background-color: rgba(255, 255, 255, 0.07);
}`,le={settingsPanel:"styles-module__settingsPanel___qNkn-",settingsHeader:"styles-module__settingsHeader___Fn1DP",settingsBrand:"styles-module__settingsBrand___OoKlM",settingsVersion:"styles-module__settingsVersion___rXmL9",settingsSection:"styles-module__settingsSection___n5V-4",settingsLabel:"styles-module__settingsLabel___VCVOQ",cycleButton:"styles-module__cycleButton___XMBx3",cycleDot:"styles-module__cycleDot___zgSXY",dropdownButton:"styles-module__dropdownButton___mKHe8",sliderLabel:"styles-module__sliderLabel___6K5v1",slider:"styles-module__slider___v5z-c",themeToggle:"styles-module__themeToggle___3imlT",below:"styles-module__below___Vpv-k",settingsOption:"styles-module__settingsOption___JoyH-",selected:"styles-module__selected___k1-Vq",settingsPanelContainer:"styles-module__settingsPanelContainer___5it-H",settingsPage:"styles-module__settingsPage___BMn-3",slideLeft:"styles-module__slideLeft___qUvW4",automationsPage:"styles-module__automationsPage___N7By0",slideIn:"styles-module__slideIn___uXDSu",themeIconWrapper:"styles-module__themeIconWrapper___pyaYa",themeIcon:"styles-module__themeIcon___w7lAm",themeIconIn:"styles-module__themeIconIn___qUWMV",settingsSectionGrow:"styles-module__settingsSectionGrow___eZTRw",settingsRow:"styles-module__settingsRow___y-tDE",settingsRowMarginTop:"styles-module__settingsRowMarginTop___uLpGb",settingsRowDisabled:"styles-module__settingsRowDisabled___ydl3Q",cycleButtonText:"styles-module__cycleButtonText___mbbnD",cycleTextIn:"styles-module__cycleTextIn___VBNTi",cycleDots:"styles-module__cycleDots___ehp6i",active:"styles-module__active___dpAhM",colorOptions:"styles-module__colorOptions___pbxZx",colorOption:"styles-module__colorOption___Co955",settingsNavLink:"styles-module__settingsNavLink___uYIwM",settingsNavLinkRight:"styles-module__settingsNavLinkRight___XBUzC",settingsBackButton:"styles-module__settingsBackButton___fflll",automationHeader:"styles-module__automationHeader___Avra9",automationDescription:"styles-module__automationDescription___vFTmJ",learnMoreLink:"styles-module__learnMoreLink___cG7OI",autoSendContainer:"styles-module__autoSendContainer___VpkXk",autoSendLabel:"styles-module__autoSendLabel___ngNdC",disabled:"styles-module__disabled___9AZYS",mcpStatusDot:"styles-module__mcpStatusDot___8AMxP",connecting:"styles-module__connecting___QEO1r",mcpPulse:"styles-module__mcpPulse___5Q3Jj",connected:"styles-module__connected___WyFkx",disconnected:"styles-module__disconnected___mvmvQ",mcpPulseError:"styles-module__mcpPulseError___VHxhx",mcpNavIndicator:"styles-module__mcpNavIndicator___auBHI",webhookUrlInput:"styles-module__webhookUrlInput___WDDDC",checkboxField:"styles-module__checkboxField___ZrSqv",divider:"styles-module__divider___h6Yux",scaleIn:"styles-module__scaleIn___QpQ8E"};var oe=ce(ze()),W1=(0,Sr.memo)(function({settings:t,onSettingsChange:n,isDarkMode:o,onToggleTheme:r,isDevMode:i,connectionStatus:s,endpoint:l,onExited:a,isOpen:c,toolbarNearBottom:d,settingsPage:m,onSettingsPageChange:f,onHideToolbar:k}){let{ref:w}=Vc(c,{keepMounted:!0,onExited:a}),C=(0,Sr.useRef)(null),S=(0,Sr.useRef)(null),g=(0,Sr.useRef)(!1);(0,Sr.useLayoutEffect)(()=>{!c||!g.current||(g.current=!1,(m==="automations"?S:C).current?.focus())},[c,m]);let x=o?"Switch to light mode":"Switch to dark mode";return(0,oe.jsx)("div",{className:`${le.settingsPanel} ${d?le.below:""}`,style:d?{bottom:"auto",top:"calc(100% + 0.5rem)"}:void 0,"data-agentation-settings-panel":!0,ref:h=>{w.current=h,h?.toggleAttribute("inert",!c)},role:"group","aria-label":"Feedback settings","aria-hidden":!c,children:(0,oe.jsxs)("div",{className:le.settingsPanelContainer,children:[(0,oe.jsxs)("div",{className:`${le.settingsPage} ${m==="automations"?le.slideLeft:""}`,ref:h=>{h?.toggleAttribute("inert",m!=="main")},"aria-hidden":m!=="main",children:[(0,oe.jsxs)("div",{className:le.settingsHeader,children:[(0,oe.jsx)("a",{className:le.settingsBrand,href:"https://agentation.com",target:"_blank",rel:"noopener noreferrer","aria-label":"Agentation",children:"Agentation"}),(0,oe.jsxs)("p",{className:le.settingsVersion,children:["v","3.1.2"]}),(0,oe.jsx)("button",{className:le.themeToggle,onClick:r,title:x,"aria-label":x,children:(0,oe.jsx)("span",{className:le.themeIconWrapper,children:(0,oe.jsx)("span",{className:le.themeIcon,children:o?(0,oe.jsx)(ry,{size:20}):(0,oe.jsx)(iy,{size:20})},o?"sun":"moon")})})]}),(0,oe.jsx)("div",{className:le.divider}),(0,oe.jsxs)("div",{className:le.settingsSection,children:[(0,oe.jsxs)("div",{className:le.settingsRow,children:[(0,oe.jsxs)("div",{className:le.settingsLabel,children:["Output Detail",(0,oe.jsx)(Ro,{content:"Controls how much detail is included in the copied output"})]}),(0,oe.jsxs)("button",{className:le.cycleButton,onClick:()=>{let E=(cs.findIndex(F=>F.value===t.outputDetail)+1)%cs.length;n({outputDetail:cs[E].value})},children:[(0,oe.jsx)("span",{className:le.cycleButtonText,children:cs.find(h=>h.value===t.outputDetail)?.label},t.outputDetail),(0,oe.jsx)("span",{className:le.cycleDots,children:cs.map(h=>(0,oe.jsx)("span",{className:`${le.cycleDot} ${t.outputDetail===h.value?le.active:""}`},h.value))})]})]}),(0,oe.jsxs)("div",{className:`${le.settingsRow} ${le.settingsRowMarginTop} ${i?"":le.settingsRowDisabled}`,children:[(0,oe.jsxs)("div",{className:le.settingsLabel,children:["React Components",(0,oe.jsx)(Ro,{content:i?"Include React component names in annotations":"Disabled \u2014 production builds minify component names, making detection unreliable. Use in development mode."})]}),(0,oe.jsx)(lu,{"aria-label":"React Components",checked:i&&t.reactEnabled,onChange:h=>n({reactEnabled:h.target.checked}),disabled:!i})]}),(0,oe.jsxs)("div",{className:`${le.settingsRow} ${le.settingsRowMarginTop}`,children:[(0,oe.jsxs)("div",{className:le.settingsLabel,children:["Hide Until Restart",(0,oe.jsx)(Ro,{content:"Hides the toolbar until you open a new tab"})]}),(0,oe.jsx)(lu,{"aria-label":"Hide Until Restart",checked:!1,onChange:h=>{h.target.checked&&k()}})]})]}),(0,oe.jsx)("div",{className:le.divider}),(0,oe.jsxs)("div",{className:le.settingsSection,children:[(0,oe.jsx)("div",{className:`${le.settingsLabel} ${le.settingsLabelMarker}`,children:"Marker Color"}),(0,oe.jsx)("div",{className:le.colorOptions,children:ps.map(h=>(0,oe.jsx)("button",{className:`${le.colorOption} ${t.annotationColorId===h.id?le.selected:""}`,style:{"--swatch":h.srgb,"--swatch-p3":h.p3},onClick:()=>n({annotationColorId:h.id}),title:h.label,"aria-label":h.label,type:"button"},h.id))})]}),(0,oe.jsx)("div",{className:le.divider}),(0,oe.jsxs)("div",{className:le.settingsSection,children:[(0,oe.jsx)(Xm,{className:"checkbox-field",label:"Clear on copy/send",checked:t.autoClearAfterCopy,onChange:h=>n({autoClearAfterCopy:h.target.checked}),tooltip:"Automatically clear annotations after copying"}),(0,oe.jsx)(Xm,{className:le.checkboxField,label:"Block page interactions",checked:t.blockInteractions,onChange:h=>n({blockInteractions:h.target.checked})})]}),(0,oe.jsx)("div",{className:le.divider}),(0,oe.jsxs)("button",{className:le.settingsNavLink,ref:C,onClick:h=>{g.current=h.detail===0,h.currentTarget.blur(),f("automations")},children:[(0,oe.jsx)("span",{children:"Manage MCP & Webhooks"}),(0,oe.jsxs)("span",{className:le.settingsNavLinkRight,children:[l&&s!=="disconnected"&&(0,oe.jsx)("span",{className:`${le.mcpNavIndicator} ${le[s]}`}),(0,oe.jsx)("svg",{width:"16",height:"16",viewBox:"0 0 16 16",fill:"none",xmlns:"http://www.w3.org/2000/svg",children:(0,oe.jsx)("path",{d:"M7.5 12.5L12 8L7.5 3.5",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})})]})]})]}),(0,oe.jsxs)("div",{className:`${le.settingsPage} ${le.automationsPage} ${m==="automations"?le.slideIn:""}`,ref:h=>{h?.toggleAttribute("inert",m!=="automations")},"aria-hidden":m!=="automations",children:[(0,oe.jsxs)("button",{className:le.settingsBackButton,ref:S,"aria-label":"Back to settings",onClick:h=>{g.current=h.detail===0,h.currentTarget.blur(),f("main")},children:[(0,oe.jsx)(ly,{size:16}),(0,oe.jsx)("span",{children:"Manage MCP & Webhooks"})]}),(0,oe.jsx)("div",{className:le.divider}),(0,oe.jsxs)("div",{className:le.settingsSection,children:[(0,oe.jsxs)("div",{className:le.settingsRow,children:[(0,oe.jsxs)("span",{className:le.automationHeader,children:["MCP Connection",(0,oe.jsx)(Ro,{content:"Connect via Model Context Protocol to let AI agents like Claude Code receive annotations in real-time."})]}),l&&(0,oe.jsx)("div",{className:`${le.mcpStatusDot} ${le[s]}`,title:s==="connected"?"Connected":s==="connecting"?"Connecting...":"Disconnected"})]}),(0,oe.jsxs)("p",{className:le.automationDescription,style:{paddingBottom:6},children:["MCP connection allows agents to receive and act on annotations."," ",(0,oe.jsx)("a",{href:"https://agentation.com/mcp",target:"_blank",rel:"noopener noreferrer",className:le.learnMoreLink,children:"Learn more"})]})]}),(0,oe.jsx)("div",{className:le.divider}),(0,oe.jsxs)("div",{className:`${le.settingsSection} ${le.settingsSectionGrow}`,children:[(0,oe.jsxs)("div",{className:le.settingsRow,children:[(0,oe.jsxs)("span",{className:le.automationHeader,children:["Webhooks",(0,oe.jsx)(Ro,{content:"Send annotation data to any URL endpoint when annotations change. Useful for custom integrations."})]}),(0,oe.jsxs)("div",{className:le.autoSendContainer,children:[(0,oe.jsx)("label",{htmlFor:"agentation-auto-send",className:`${le.autoSendLabel} ${t.webhooksEnabled?le.active:""} ${t.webhookUrl?"":le.disabled}`,children:"Auto-Send"}),(0,oe.jsx)(lu,{id:"agentation-auto-send",checked:t.webhooksEnabled,onChange:h=>n({webhooksEnabled:h.target.checked}),disabled:!t.webhookUrl})]})]}),(0,oe.jsx)("p",{className:le.automationDescription,children:"The webhook URL will receive live annotation changes and annotation data."}),(0,oe.jsx)("textarea",{className:le.webhookUrlInput,placeholder:"Webhook URL","aria-label":"Webhook URL",value:t.webhookUrl,onKeyDown:h=>h.stopPropagation(),onChange:h=>n({webhookUrl:h.target.value})})]})]})]})})});var au=ce(vt());var Yl=ce(ze());function U1({x:e,y:t,elementName:n,reactComponents:o}){let r=(0,au.useRef)(null);return(0,au.useLayoutEffect)(()=>{let i=r.current;if(!i)return;let s=()=>{let l=i.offsetWidth,a=i.offsetHeight;i.style.left=`${Math.max(8,Math.min(e,window.innerWidth-l-8))}px`;let c=t-a-8;i.style.top=`${Math.max(8,Math.min(c,window.innerHeight-a-8))}px`};return s(),window.addEventListener("resize",s),()=>window.removeEventListener("resize",s)},[e,t,n,o]),(0,Yl.jsxs)("div",{ref:r,className:`${z.hoverTooltip} ${z.enter}`,children:[o&&(0,Yl.jsx)("div",{className:z.hoverReactPath,children:o}),(0,Yl.jsx)("div",{className:z.hoverElementName,children:n})]})}var j1=`@charset "UTF-8";
/* Reset box-model and set borders */
/* ============================================ */
*,
::before,
::after {
  border-width: 0;
  border-style: solid;
  box-sizing: border-box;
}

/* Document */
/* ============================================ */
/**
 * 1. Correct line height in all browsers.
 * 2. Prevent adjustments of font size after orientation changes in iOS.
 * 3. Remove gray overlay on links for iOS.
 * 4. Render kerning consistently in all browsers.
 * 5. Correct font smoothing for macOS.
 */
:host {
  /* Inherited properties cross the shadow boundary, so a host page's
     text-transform, letter-spacing or font would otherwise restyle the UI. */
  font: 400 16px/1.5 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  font-variant: normal;
  color: initial;
  letter-spacing: normal;
  word-spacing: normal;
  text-transform: none;
  text-align: start;
  text-indent: 0;
  text-shadow: none;
  white-space: normal;
  direction: ltr;
  writing-mode: horizontal-tb;
  hyphens: manual;
  word-break: normal;
  overflow-wrap: normal;
  tab-size: 8;
  list-style: none;
  quotes: initial;
  caret-color: auto;
  user-select: auto;
  -webkit-text-fill-color: initial;
  -webkit-text-stroke: 0;
  text-rendering: auto;
  -webkit-text-size-adjust: 100%; /* 2 */
  -webkit-tap-highlight-color: transparent; /* 3 */
  font-feature-settings: "kern"; /* 4 */
  -webkit-font-feature-settings: "kern"; /* 5 */
  -moz-font-feature-settings: "kern"; /* 5 */
  -webkit-font-smoothing: antialiased; /* 5 */
  -moz-osx-font-smoothing: grayscale; /* 5 */
}

/* Vertical rhythm */
/* ============================================ */
p,
table,
blockquote,
address,
pre,
iframe,
form,
figure,
dl {
  margin: 0;
}

/* Headings */
/* ============================================ */
h1,
h2,
h3,
h4,
h5,
h6 {
  margin: 0;
  font-size: inherit;
  font-weight: inherit;
}

/* Lists (enumeration) */
/* ============================================ */
ul,
ol,
menu {
  list-style: none;
  margin: 0;
  padding: 0;
}

/* Lists (definition) */
/* ============================================ */
dd {
  margin-left: 0;
}

/* Grouping content */
/* ============================================ */
/**
 * 1. Add the correct box sizing in Firefox.
 * 2. Show the overflow in Edge and IE.
 */
hr {
  clear: both;
  margin: 0;
  border-top-width: 1px;
  height: 0; /* 1 */
  box-sizing: content-box; /* 1 */
  overflow: visible; /* 2 */
  color: inherit;
}

/**
 * 1. Correct the inheritance and scaling of font size in all browsers.
 * 2. Correct the odd \`em\` font sizing in all browsers.
 * 3. Wrap lines by default instead of overflow.
 */
pre {
  font-family: inherit; /* 1 */
  font-size: inherit; /* 2 */
  white-space: pre-line; /* 3 */
}

address {
  font-style: inherit;
}

/* Text-level semantics */
/* ============================================ */
/**
 * Remove the gray background on active links in IE 10.
 */
a {
  background-color: transparent;
  text-decoration: none;
  color: inherit;
}

/**
 * 1. Remove the bottom border in Chrome 57-
 * 2. Add the correct text decoration in Chrome, Edge, IE, Opera, and Safari.
 */
abbr[title] {
  border-bottom: none; /* 1 */
  text-decoration: none; /* 2 */
}

/**
 * Add the correct font weight in Chrome, Edge, and Safari.
 */
b,
strong {
  font-weight: bolder;
}

/**
 * 1. Correct the inheritance and scaling of font size in all browsers.
 * 2. Correct the odd \`em\` font sizing in all browsers.
 */
code,
kbd,
samp {
  font-family: "Menlo", "Monaco", "Consolas", "Courier New", monospace; /* 1 */
  font-size: inherit; /* 2 */
}

/**
 * Add the correct font size in all browsers.
 */
small {
  font-size: 80%;
}

/**
 * Prevent \`sub\` and \`sup\` elements from affecting the line height in all browsers.
 */
sub,
sup {
  position: relative;
  vertical-align: baseline;
  line-height: 0;
  font-size: 75%;
}

sub {
  bottom: -0.25em;
}

sup {
  top: -0.5em;
}

/* Replaced content */
/* ============================================ */
/**
 * Prevent vertical alignment issues.
 */
svg,
img,
embed,
object,
iframe {
  vertical-align: bottom;
}

/*
 * 1. Remove image default bottom space.
 * 2. Prevent image from overflowing the container.
 */
img {
  display: block;
  max-width: 100%;
}

/**
 * Prevent alignment issues on Safari.
 */
@supports (background: -webkit-named-image(i)) {
  svg {
    will-change: transform;
  }
}
/* Forms */
/* ============================================ */
/**
 * Reset form fields to make them styleable.
 * 1. Make form elements stylable across systems iOS especially.
 * 2. Inherit text-transform from parent.
 */
button,
input,
optgroup,
select,
textarea {
  -webkit-appearance: none; /* 1 */
  appearance: none;
  border-radius: 0;
  margin: 0;
  padding: 0;
  background: transparent;
  vertical-align: middle;
  text-align: inherit;
  text-transform: inherit; /* 2 */
  font: inherit;
  color: inherit;
}

/**
 * Correct cursors for clickable elements.
 */
button,
[type=button],
[type=reset],
[type=submit] {
  cursor: pointer;
}

button:disabled,
[type=button]:disabled,
[type=reset]:disabled,
[type=submit]:disabled {
  cursor: default;
}

/**
 * Clickable labels and selects.
 */
select,
label {
  cursor: pointer;
}

/**
 * Improve outlines for Firefox and unify style with input elements & buttons.
 */
:-moz-focusring {
  outline: auto;
}

select:disabled {
  opacity: inherit;
}

/**
 * 1. Remove padding.
 */
option {
  padding: 0; /* 1 */
}

/**
 * Reset to invisible
 */
fieldset {
  margin: 0;
  padding: 0;
  min-width: 0;
}

legend {
  display: contents;
  padding: 0;
}

/**
 * Add the correct vertical alignment in Chrome, Firefox, and Opera.
 */
progress {
  vertical-align: baseline;
}

/**
 * Remove the default vertical scrollbar in IE 10+.
 */
textarea {
  overflow: auto;
}

/**
 * Remove increment and decrement buttons in Chrome.
 */
[type=number]::-webkit-inner-spin-button,
[type=number]::-webkit-outer-spin-button {
  -webkit-appearance: none;
}

/**
 * Correct the outline style in Safari.
 */
[type=search] {
  outline-offset: -2px;
}

/**
 * Remove the inner padding in Chrome and Safari on macOS.
 */
[type=search]::-webkit-search-decoration {
  -webkit-appearance: none;
}

/*
 * Remove the \u2018X\u2019 from Chrome and Safari.
 */
[type=search]::-webkit-search-decoration,
[type=search]::-webkit-search-cancel-button,
[type=search]::-webkit-search-results-button,
[type=search]::-webkit-search-results-decoration {
  display: none;
}

/**
 * 1. Hide file input completely.
 * 2. Remove selected file text.
 * 3. Set cursor to pointer for all browsers.
 */
[type=file] {
  opacity: 0; /* 1 */
  font-size: 0; /* 2 */
  cursor: pointer; /* 3 */
}

/**
	* Fix appearance for Firefox
	*/
[type=number] {
  -moz-appearance: textfield;
}

/**
 * Set cursor to pointer for all browsers.
 */
[type=range] {
  cursor: pointer;
}

/**
 * Reset slider thumbs to make them styleable.
 */
[type=range]::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
}

[type=range]::-moz-range-thumb {
  -moz-appearance: none;
  appearance: none;
  border-width: 0;
  border-radius: 0;
  background-color: transparent;
}

/* Interactive */
/* ============================================ */
/*
 * Add the correct display in Edge, IE 10+, and Firefox.
 */
details {
  display: block;
}

/*
 * Add the correct display in all browsers.
 */
summary {
  display: list-item;
}

/*
 * Remove outline for editable content.
 */
[contenteditable]:focus {
  outline: auto;
}

/* Tables */
/* ============================================ */
/**
1. Correct table border color inheritance in all Chrome and Safari.
*/
table {
  border-color: inherit; /* 1 */
  border-collapse: collapse;
}

caption {
  text-align: left;
}

td,
th {
  vertical-align: top;
  padding: 0;
}

th {
  text-align: left;
  font-weight: inherit;
}

/* Misc */
/* ============================================ */
/*
 * Make placeholder style consistent across all browsers.
 */
::placeholder {
  color: #999;
  opacity: 1;
}

/*
 * Hide focus outline but keep it visible for Windows High Contrast Mode.
 */
:focus {
  outline-style: solid;
  outline-color: transparent;
}

/*
 * Hide input arrow when used with datalist.
 */
::-webkit-calendar-picker-indicator {
  display: none !important;
}`;var X=ce(ze()),Q1=ce(vt()),vk=[j1,_y,Wc,F1,fy,dy,ss,$1,N1,B1,H1,A1].join(`
`);function cu(e,t="filtered",n){let{name:o,path:r}=vr(e,n);if(t==="off")return{name:o,elementName:o,path:r,reactComponents:null};let i=e1(e,{mode:t});return{name:i.path?`${i.path} ${o}`:o,elementName:o,path:r,reactComponents:i.path}}var Y1=!1,Qm={outputDetail:"standard",autoClearAfterCopy:!1,annotationColorId:"blue",blockInteractions:!0,reactEnabled:!0,markerClickBehavior:"edit",webhookUrl:"",webhooksEnabled:!0},V1=e=>{if(!e||!e.trim())return!1;try{let t=new URL(e.trim());return t.protocol==="http:"||t.protocol==="https:"}catch{return!1}},wk={compact:"off",standard:"filtered",detailed:"smart",forensic:"all"},Cr=e=>e.metaKey||e.ctrlKey,ps=[{id:"indigo",label:"Indigo",srgb:"#6155F5",p3:"color(display-p3 0.38 0.33 0.96)"},{id:"blue",label:"Blue",srgb:"#0088FF",p3:"color(display-p3 0.00 0.53 1.00)"},{id:"cyan",label:"Cyan",srgb:"#00C3D0",p3:"color(display-p3 0.00 0.76 0.82)"},{id:"green",label:"Green",srgb:"#34C759",p3:"color(display-p3 0.20 0.78 0.35)"},{id:"yellow",label:"Yellow",srgb:"#FFCC00",p3:"color(display-p3 1.00 0.80 0.00)"},{id:"orange",label:"Orange",srgb:"#FF8D28",p3:"color(display-p3 1.00 0.55 0.16)"},{id:"red",label:"Red",srgb:"#FF383C",p3:"color(display-p3 1.00 0.22 0.24)"}],bk=[...ps.map(e=>`
    [data-agentation-accent="${e.id}"] {
      --agentation-color-accent: ${e.srgb};
    }
    @supports (color: color(display-p3 0 0 0)) {
      [data-agentation-accent="${e.id}"] {
        --agentation-color-accent: ${e.p3};
      }
    }
  `),`:host {
    ${ps.map(e=>`--agentation-color-${e.id}: ${e.srgb};`).join(`
`)}
  }`,`@supports (color: color(display-p3 0 0 0)) {
    :host {
      ${ps.map(e=>`--agentation-color-${e.id}: ${e.p3};`).join(`
`)}
    }
  }`].join("");function Gm(e){let t=e;for(let o=yn(t.ownerDocument);o;o=yn(t.ownerDocument))t=o;let n=t;for(;n&&n!==document.body;){let r=window.getComputedStyle(n).position;if(r==="fixed"||r==="sticky")return!0;n=n.parentElement}return!1}function Vl(e){return e.kind!=="placement"&&e.kind!=="rearrange"&&e.status!=="resolved"&&e.status!=="dismissed"}function uu(e){let t=tu(e),n=t.found?t:n1(e);if(n.found&&n.source)return t1(n.source,"path")}function qm(e={}){let t=A0(e.useHashLocation??!1),n=(0,T.useState)(!1),o=z0(e.portalContainer);return o?(0,X1.createPortal)((0,Q1.createElement)(kk,{...e,key:e.useHashLocation?t:void 0,pathname:t,activeState:n,portalHost:o}),o):null}function kk({pathname:e,activeState:t,portalHost:n,useHashLocation:o=!1,appName:r,enableKeyboardShortcuts:i=!0,identifyingAttributes:s=Oc,copyFormat:l="markdown",onOpenSource:a,portalContainer:c,demoAnnotations:d,demoDelay:m=1e3,enableDemoMode:f=!1,onAnnotationAdd:k,onAnnotationDelete:w,onAnnotationUpdate:C,onAnnotationsClear:S,onCopy:g,onSubmit:x,copyToClipboard:h=!0,endpoint:E,sessionId:F,onSessionCreated:W,webhookUrl:D,className:j,captureScreenshots:Z=!0,screenshotReference:q}){let[O,Ae]=t,[,ue]=(0,T.useState)(0),[J]=(0,T.useState)(()=>L0(document,()=>ue(p=>p+1)));(0,T.useEffect)(()=>(J.start(),()=>J.stop()),[J]);let Ce=typeof l=="object"?l.attribute:void 0,K=(0,T.useMemo)(()=>Ce?[...s,Ce]:s,[s,Ce]),Ge=(0,T.useRef)(!0);(0,T.useLayoutEffect)(()=>(Ge.current=!0,()=>{Ge.current=!1}),[]);let Le=(0,T.useCallback)(p=>o?F0(JSON.stringify([E,e]),p):p(),[E,e,o]),Ye=(0,T.useRef)(new Map),xe=(0,T.useRef)(new Set),gt=p=>Vl(p)&&!xe.current.has(p.id),Ft=async(...p)=>{let y=await Dm(...p),b=p[2],L=b.id;if(L&&(Ye.current.set(L,y.id),xe.current.has(L)&&xe.current.add(y.id)),!o){let N=new URL(b.url||window.location.href).pathname,P=Xo(N).find(I=>I.id===L);try{if(xe.current.has(L))await Dl(p[0],y.id);else if(P&&P.comment!==b.comment)return await Zc(p[0],y.id,{comment:P.comment}),{...y,comment:P.comment}}catch(I){console.warn("[Agentation] Failed to apply changes made during sync:",I)}}return y},Kn=p=>o?{...p,annotations:p.annotations.filter(y=>!xe.current.has(y.id)&&O0(y.url||p.url,e,window.location.origin))}:p,_n=(0,T.useRef)(e);_n.current=e;let Jn=(p,y,b,L=e)=>{let N=S0(p,Xo(L),y,Ye.current).filter(gt);L===_n.current&&Ge.current&&vn(N),qc(L,N,b)},[Ve,vn]=(0,T.useState)([]),[po,fo]=(0,T.useState)(!0),[Qo,Go]=(0,T.useState)(()=>qy()),[On,Er]=(0,T.useState)(!1);(0,T.useLayoutEffect)(()=>{am()},[]);let Ot=(0,T.useRef)(null),Zn=(0,T.useRef)(null),go=(0,T.useRef)(null),Mr=(0,T.useRef)(null),G=(0,T.useRef)(!1),_e=(0,T.useRef)(!1),Ee=(0,T.useRef)(!1);(0,T.useLayoutEffect)(()=>{O&&G.current?(G.current=!1,go.current?.querySelector("button:not(:disabled)")?.focus()):!O&&_e.current&&(_e.current=!1,Zn.current?.focus())},[O]),(0,T.useEffect)(()=>{let p=b=>{let L=Ot.current;L&&b.composedPath().includes(L)&&b.stopPropagation()},y=["mousedown","click","pointerdown"];return y.forEach(b=>n.addEventListener(b,p)),()=>{y.forEach(b=>n.removeEventListener(b,p))}},[n]);let[Pe,ke]=(0,T.useState)(!1),[dt,rt]=(0,T.useState)(!1),[Fe,$e]=(0,T.useState)(null),[Et,R]=(0,T.useState)({x:0,y:0}),[M,B]=(0,T.useState)(null),[U,re]=(0,T.useState)(!1),ae=um(),Y=um(),[ie,ve]=(0,T.useState)("idle"),[Oe,it]=(0,T.useState)(!1),Ze=(0,T.useRef)(new Set),_t=(0,T.useRef)(new Set),je=(0,T.useRef)(),Se=(0,T.useCallback)(()=>{!Ze.current.size&&!je.current&&it(!1)},[]);(0,T.useEffect)(()=>()=>clearTimeout(je.current),[]);let[ut,Mt]=(0,T.useState)(null),[st,Ne]=(0,T.useState)(null),[et,lt]=(0,T.useState)([]),[yt,Yt]=(0,T.useState)(null),wn=(0,T.useRef)(null);(0,T.useEffect)(()=>()=>{wn.current&&clearTimeout(wn.current)},[]);let[me,Kt]=(0,T.useState)(null),Ln=(0,T.useRef)(null),Tr=(0,T.useRef)(!1),[eo,ho]=(0,T.useState)(!1);(0,T.useLayoutEffect)(()=>{if(me||!Ln.current)return;let p=Ln.current;if(Ln.current=null,M)return;(O&&p.isConnected&&!p.disabled?p:Zn.current)?.focus({preventScroll:!0})},[me,O,M]);let[ai,Lr]=(0,T.useState)(null),[Km,fs]=(0,T.useState)([]),[Rr,Jm]=(0,T.useState)(0),[Zm,ep]=(0,T.useState)(!1),[at,K1]=(0,T.useState)(!1),[Bn,tp]=(0,T.useState)(!1),[nn,Pr]=(0,T.useState)(!1),[J1,np]=(0,T.useState)("main"),[op,du]=(0,T.useState)(!1),[qe,_u]=(0,T.useState)(!1),[Nr,gs]=(0,T.useState)(!1),[Be,Io]=(0,T.useState)([]),[ci,Ir]=(0,T.useState)(null),mu=(0,T.useRef)(!1),[xt,rp]=(0,T.useState)(!1),[ip,pu]=(0,T.useState)(!1),[sp,Z1]=(0,T.useState)(1),[lp,Ek]=(0,T.useState)("new-page"),[on,Xl]=(0,T.useState)(""),[e5,t5]=(0,T.useState)(!1),[pe,yo]=(0,T.useState)(null),fu=(0,T.useRef)(!1),gu=(0,T.useRef)({rearrange:null,placements:[]}),Dr=(0,T.useRef)({rearrange:null,placements:[]}),[n5,ap]=(0,T.useState)(0),[o5,r5]=(0,T.useState)(0),[ui,cp]=(0,T.useState)([]),[di,up]=(0,T.useState)(null),dp=(0,T.useRef)({designPlacements:Be,rearrangeState:pe,blankCanvas:xt,wireframePurpose:on});dp.current={designPlacements:Be,rearrangeState:pe,blankCanvas:xt,wireframePurpose:on};let _i=(0,T.useRef)({placements:ui,rearrange:di}),hs=(0,T.useRef)(new Set),Ql=(0,T.useRef)(new Set),to=(0,T.useRef)(null),mi=(0,T.useRef)(),_p=qe&&O&&!Nr&&xt;(0,T.useEffect)(()=>{if(_p){pu(!1);let p=Gi(()=>{pu(!0)});return()=>cancelAnimationFrame(p)}else pu(!1)},[_p]);let hu=(0,T.useRef)(new Map),yu=(0,T.useRef)([]),xu=(0,T.useRef)(new Map),$r=(0,T.useRef)(null),[zn,vu]=(0,T.useState)(!1),[no,i5]=(0,T.useState)([]),wu=(0,T.useRef)(no);wu.current=no;let[mp,Mk]=(0,T.useState)(null),bu=(0,T.useRef)(null),Tk=(0,T.useRef)(!1),Lk=(0,T.useRef)([]),Rk=(0,T.useRef)(0),Pk=(0,T.useRef)(null),Nk=(0,T.useRef)(null),Ik=(0,T.useRef)(1),[ku,pp]=(0,T.useState)(!1),pi=(0,T.useRef)(null),[bn,qo]=(0,T.useState)([]),ys=(0,T.useRef)(!1),rn=()=>{du(!0)},s5=()=>{du(!1)},fp=()=>{ku||(pi.current=De(()=>pp(!0),850))},gp=()=>{pi.current&&(clearTimeout(pi.current),pi.current=null),pp(!1),s5()};(0,T.useEffect)(()=>()=>{pi.current&&clearTimeout(pi.current)},[]);let[tt,l5]=(0,T.useState)(()=>{try{let p=JSON.parse(localStorage.getItem("feedback-toolbar-settings")??"");return{...Qm,...p,annotationColorId:ps.find(y=>y.id===p.annotationColorId)?p.annotationColorId:Qm.annotationColorId}}catch{return Qm}}),[xo,hp]=(0,T.useState)(!0),[yp,xp]=(0,T.useState)(!1),a5=(0,T.useCallback)(p=>{l5(y=>({...y,...p}))},[]),c5=(0,T.useCallback)(()=>{Ot.current?.classList.add(z.disableTransitions),hp(p=>!p),Gi(()=>{Ot.current?.classList.remove(z.disableTransitions)})},[]),vp=!1,Do=vp&&tt.reactEnabled?wk[tt.outputDetail]:"off",[Ht,Su]=(0,T.useState)(o?null:F??null),wp=(0,T.useRef)(!1),[Ar,Fr]=(0,T.useState)(E?"connecting":"disconnected"),[Nt,Cu]=(0,T.useState)(null),[Gl,bp]=(0,T.useState)(!1),xs=(0,T.useRef)(null),ql=(0,T.useRef)(!1),Or=(0,T.useRef)(new Set),Kl=(0,T.useRef)(new Map),kp=(0,T.useCallback)(p=>{Or.current.add(p),oo.current===p&&(oo.current=null)},[]),[Ko,Jl]=(0,T.useState)(new Set),[Hn,vs]=(0,T.useState)(!1),[Br,fi]=(0,T.useState)(!1),[$o,Eu]=(0,T.useState)(!1),zr=(0,T.useRef)(null),Wn=(0,T.useRef)(null),ws=(0,T.useRef)(null),gi=(0,T.useRef)(null),bs=(0,T.useRef)(!1),Sp=(0,T.useRef)(0),oo=(0,T.useRef)(null),Cp=(0,T.useRef)(null),Mu=8,u5=50,Tu=(0,T.useRef)(null),Zl=(0,T.useRef)(null),ks=(0,T.useRef)(null),d5=(0,T.useCallback)(()=>np("main"),[]);(0,T.useEffect)(()=>{nn||du(!1)},[nn]),(0,T.useLayoutEffect)(()=>{nn&&Ee.current&&(Ee.current=!1,Ot.current?.querySelector("[data-agentation-settings-panel] button")?.focus())},[nn]);let ea=O&&po&&!qe;(0,T.useEffect)(()=>{if(ea)rt(!1),ke(!0),Or.current.clear();else if(Pe){rt(!0);let p=De(()=>{ke(!1),rt(!1)},250);return()=>clearTimeout(p)}},[ea]),(0,T.useEffect)(()=>{K1(!0),Jm(window.scrollY);let p=Xo(e);vn(p.filter(Vl)),Y1||(xp(!0),Y1=!0,De(()=>xp(!1),750));try{let y=localStorage.getItem("feedback-toolbar-theme");y!==null&&hp(y==="dark")}catch{}try{let y=localStorage.getItem("feedback-toolbar-position");if(y){let b=JSON.parse(y);typeof b.x=="number"&&typeof b.y=="number"&&Cu(b)}}catch{}},[e]),(0,T.useEffect)(()=>{at&&localStorage.setItem("feedback-toolbar-settings",JSON.stringify(tt))},[tt,at]),(0,T.useEffect)(()=>{at&&localStorage.setItem("feedback-toolbar-theme",xo?"dark":"light")},[xo,at]);let Ep=(0,T.useRef)(!1);(0,T.useEffect)(()=>{let p=Ep.current;Ep.current=Gl,p&&!Gl&&Nt&&at&&localStorage.setItem("feedback-toolbar-position",JSON.stringify(Nt))},[Gl,Nt,at]),(0,T.useEffect)(()=>{if(!E||!at||wp.current)return;wp.current=!0,Fr("connecting");let p=window.location.href;Le(async()=>{try{let b=Qy(e),L=F||b,N=!1;if(L)try{let P=Xo(e),I=Kn(await Im(E,L));yu.current=I.annotations.filter(fe=>fe.kind==="placement"||fe.kind==="rearrange"),Ge.current&&(Su(I.id),Fr("connected")),Kc(e,I.id),N=!0;let V=Xo(e).filter(Vl),ee=new Set(I.annotations.map(fe=>fe.id)),ge=V.filter(fe=>!ee.has(fe.id));if(ge.length>0){let he=`${typeof window<"u"?window.location.origin:""}${e}`,He=(await Promise.allSettled(ge.map(ye=>Ft(E,I.id,{...ye,sessionId:I.id,url:he})))).map((ye,Me)=>ye.status==="fulfilled"?ye.value:(console.warn("[Agentation] Failed to sync annotation:",ye.reason),ge[Me])),de=[...I.annotations,...He];Jn(P,de,I.id)}else Jn(P,I.annotations,I.id)}catch(P){console.warn("[Agentation] Could not join session, creating new:",P),Gy(e)}if(!N){let P=await Jc(E,p);Kc(e,P.id),Ge.current&&(Su(P.id),Fr("connected"),W?.(P.id));let I=o?new Map([[e,Xo(e)]]):By(),V=typeof window<"u"?window.location.origin:"",ee=[];for(let[ge,fe]of I){let he=fe.filter(de=>Vl(de)&&!de._syncedTo);if(he.length===0)continue;let Ie=`${V}${ge}`,He=ge===e;ee.push((async()=>{try{let de=He?P:await Jc(E,Ie),Me=(await Promise.allSettled(he.map(Ke=>Ft(E,de.id,{...Ke,sessionId:de.id,url:Ie})))).map((Ke,kt)=>Ke.status==="fulfilled"?Ke.value:(console.warn("[Agentation] Failed to sync annotation:",Ke.reason),he[kt]));Jn(he,Me,de.id,ge)}catch(de){console.warn(`[Agentation] Failed to sync annotations for ${ge}:`,de)}})())}await Promise.allSettled(ee)}}catch(b){Ge.current&&Fr("disconnected"),console.warn("[Agentation] Failed to initialize session, using local storage:",b)}})},[E,F,at,W,e,Le]),(0,T.useEffect)(()=>{if(!E||!at)return;let p=async()=>{try{(await fetch(`${E}/health`)).ok?Fr("connected"):Fr("disconnected")}catch{Fr("disconnected")}};p();let y=$c(p,1e4);return()=>clearInterval(y)},[E,at]);let ta=(0,T.useRef)(Ve),Mp=(0,T.useRef)(!1);(0,T.useLayoutEffect)(()=>{ta.current=Ve,Mp.current=Ve.length>0||Be.length>0||(pe?.sections.length??0)>0},[Ve,Be.length,pe?.sections.length]);let Lu=(0,T.useCallback)(p=>{let y=Ze.current.has(p);if(y&&(_t.current.delete(p),_t.current.size))return;let b=y?new Set(Ze.current):new Set([p]);y&&(Ze.current.clear(),Se());for(let P of b)Kl.current.delete(P),Or.current.delete(P);vn(P=>P.filter(I=>!b.has(I.id))),Jl(P=>new Set([...P].filter(I=>!b.has(I))));let L=y?[]:ta.current.filter(P=>P.kind!=="placement"&&P.kind!=="rearrange"),N=L.findIndex(P=>P.id===p);N>=0&&N<L.length-1&&(Yt(P=>P===null?N:Math.min(P,N)),wn.current&&clearTimeout(wn.current),wn.current=De(()=>Yt(null),200))},[Se]);(0,T.useEffect)(()=>!E||!at||!Ht?void 0:P1(E,Ht,()=>Mp.current,b=>{let{id:L,kind:N}=b;if(N==="placement"){for(let[P,I]of hu.current)if(I===L){$r.current?.placements.forget(P),Io(V=>V.filter(ee=>ee.id!==P));break}}else if(N==="rearrange"){for(let[P,I]of xu.current)if(I===L){$r.current?.rearrange.forget(P),yo(V=>{if(!V)return null;let ee=V.sections.filter(ge=>ge.id!==P);return ee.length===0?null:{...V,sections:ee}});break}}else{if(!ta.current.some(P=>P.id===L))return;Jl(P=>new Set(P).add(L))}}),[E,at,Ht]),(0,T.useEffect)(()=>{if(!E||!at)return;let p=Cp.current==="disconnected",y=Ar==="connected";Cp.current=Ar,p&&y&&Le(async()=>{try{let L=Xo(e).filter(Vl);if(L.length===0)return;let P=`${typeof window<"u"?window.location.origin:""}${e}`,I=Ht,V=[];if(I)try{V=Kn(await Im(E,I)).annotations}catch{I=null}I||(I=(await Jc(E,P)).id,Ge.current&&Su(I),Kc(e,I));let ee=new Set(V.map(fe=>fe.id)),ge=L.filter(fe=>!ee.has(fe.id));if(ge.length>0){let he=(await Promise.allSettled(ge.map(He=>Ft(E,I,{...He,sessionId:I,url:P})))).map((He,de)=>He.status==="fulfilled"?He.value:(console.warn("[Agentation] Failed to sync annotation on reconnect:",He.reason),ge[de])),Ie=[...V,...he];Jn(L,Ie,I)}}catch(L){console.warn("[Agentation] Failed to sync on reconnect:",L)}})},[Ar,E,at,Ht,e,Le]);let _5=(0,T.useCallback)(()=>{On||(Er(!0),Pr(!1),Ae(!1),De(()=>{Ky(!0),Go(!0),Er(!1)},400))},[On]);(0,T.useEffect)(()=>{if(!f||!at||!d||d.length===0||Ve.length>0)return;let p=[];return p.push(De(()=>{Ae(!0)},m-200)),d.forEach((y,b)=>{let L=m+b*300;p.push(De(()=>{let N=document.querySelector(y.selector);if(!N)return;let P=$t(N),{name:I,path:V}=vr(N),ee={id:`demo-${Date.now()}-${b}`,x:(P.left+P.width/2)/window.innerWidth*100,y:P.top+P.height/2+window.scrollY,comment:y.comment,element:I,elementPath:V,timestamp:Date.now(),selectedText:y.selectedText,boundingBox:{x:P.left,y:P.top+window.scrollY,width:P.width,height:P.height},nearbyText:Zi(N),cssClasses:es(N)};vn(ge=>[...ge,ee])},L))}),()=>{p.forEach(clearTimeout)}},[f,at,d,m]),(0,T.useEffect)(()=>{let p=()=>{Jm(window.scrollY),ue(y=>y+1),ep(!0),ks.current&&clearTimeout(ks.current),ks.current=De(()=>{ep(!1)},150)};return J.addEventListener("scroll",p,{passive:!0,capture:!0}),()=>{J.removeEventListener("scroll",p,!0),ks.current&&clearTimeout(ks.current)}},[J]),(0,T.useEffect)(()=>{if(!at)return;let p=Ve.filter(y=>!Ko.has(y.id));p.length>0?Ht?qc(e,p,Ht):Mm(e,p):localStorage.removeItem(Nl(e))},[Ve,e,at,Ht,Oe,Ko]),(0,T.useEffect)(()=>{if(at&&!mu.current){mu.current=!0;let p=zy(e);p.length>0&&Io(p)}},[at,e]),(0,T.useEffect)(()=>{if(at&&mu.current&&!xt){let p=Be.filter(y=>!ui.includes(y));p.length>0?Hy(e,p):Wy(e)}},[Be,e,at,xt,ui]),(0,T.useEffect)(()=>{if(at&&!fu.current){fu.current=!0;let p=Uy(e);if(p){let y={...p,sections:p.sections.map(b=>({...b,currentRect:b.currentRect??{...b.originalRect}}))};yo(y)}}},[at,e]),(0,T.useEffect)(()=>{at&&fu.current&&!xt&&(pe&&pe!==di?jy(e,pe):Yy(e))},[pe,e,at,xt,di]);let Ru=(0,T.useRef)(!1);(0,T.useEffect)(()=>{if(at&&!Ru.current){Ru.current=!0;let p=Vy(e);p&&(Dr.current={rearrange:p.rearrange,placements:p.placements||[]},p.purpose&&Xl(p.purpose))}},[at,e]),(0,T.useEffect)(()=>{if(!at||!Ru.current||Oe)return;let p=Dr.current;xt?(pe?.sections?.length??0)>0||Be.length>0||on?Pm(e,{rearrange:pe,placements:Be,purpose:on}):Il(e):(p.rearrange?.sections?.length??0)>0||p.placements.length>0||on?Pm(e,{rearrange:p.rearrange,placements:p.placements,purpose:on}):Il(e)},[pe,Be,on,xt,e,at,Oe]),(0,T.useEffect)(()=>{qe&&!pe&&yo({sections:[],originalOrder:[],detectedAt:Date.now()})},[qe,pe]),(0,T.useEffect)(()=>{if(!E||!Ht)return;let p={create:b=>Le(()=>Dm(E,Ht,b)),update:(b,L)=>Le(()=>Zc(E,b,L)),remove:b=>Le(()=>Dl(E,b))};hu.current=new Map,xu.current=new Map;let y={placements:Um(p,hu.current,yu.current.filter(b=>b.kind==="placement")),rearrange:Um(p,xu.current,yu.current.filter(b=>b.kind==="rearrange"))};return $r.current=y,()=>{y.placements.dispose(),y.rearrange.dispose(),$r.current===y&&($r.current=null)}},[E,Ht,e,Le]),(0,T.useEffect)(()=>{let p=window.location.pathname+window.location.search+window.location.hash;$r.current?.placements.replace(Be.filter(y=>!ui.includes(y)).map(y=>({id:y.id,x:y.x/window.innerWidth*100,y:y.y,comment:`Place ${y.type} at (${Math.round(y.x)}, ${Math.round(y.y)}), ${y.width}\xD7${y.height}px${y.text?` \u2014 "${y.text}"`:""}`,element:`[design:${y.type}]`,elementPath:"[placement]",timestamp:y.timestamp,url:p,intent:"change",severity:"important",kind:"placement",placement:{componentType:y.type,width:y.width,height:y.height,scrollY:y.scrollY,text:y.text}})))},[Be,E,Ht,e,ui]),(0,T.useEffect)(()=>{let p=$r.current;if(!p)return;if(pe===di){p.rearrange.replace([]);return}let y=De(()=>{let b=window.location.pathname+window.location.search+window.location.hash,L=[];for(let N of pe?.sections??[]){let P=N.originalRect,I=N.currentRect,V=Math.abs(P.x-I.x)>1||Math.abs(P.y-I.y)>1||Math.abs(P.width-I.width)>1||Math.abs(P.height-I.height)>1;if(!V&&!N.note)continue;let ee=N.note?` \u2014 "${N.note}"`:"";L.push({id:N.id,x:I.x/window.innerWidth*100,y:I.y,comment:V?`Move ${N.label} section (${N.tagName}) \u2014 from (${Math.round(P.x)},${Math.round(P.y)}) ${Math.round(P.width)}\xD7${Math.round(P.height)} to (${Math.round(I.x)},${Math.round(I.y)}) ${Math.round(I.width)}\xD7${Math.round(I.height)}${ee}`:`Note on ${N.label} section (${N.tagName})${ee}`,element:N.selector,elementPath:"[rearrange]",timestamp:pe.detectedAt,url:b,intent:"change",severity:"important",kind:"rearrange",rearrange:{selector:N.selector,label:N.label,tagName:N.tagName,originalRect:P,currentRect:I}})}p.rearrange.replace(L)},300);return()=>clearTimeout(y)},[pe,E,Ht,e,di]);let Pu=(0,T.useCallback)(()=>{clearTimeout(mi.current),gs(!1),_u(!0)},[]);(0,T.useEffect)(()=>()=>clearTimeout(mi.current),[]);let Ss=(0,T.useCallback)(()=>{gs(!0),_u(!1),Ir(null),clearTimeout(mi.current),mi.current=De(()=>{gs(!1)},300)},[]),na=(0,T.useCallback)(()=>{let p=Zn.current?.getRootNode();_e.current=!!p?.activeElement&&!!Ot.current?.contains(p.activeElement),_e.current&&p?.activeElement?.blur(),Pr(!1),qe&&(gs(!0),_u(!1),Ir(null),clearTimeout(mi.current),mi.current=De(()=>{gs(!1)},300)),Ae(!1)},[qe]),Tp=(0,T.useCallback)(()=>{Bn||(k0(),tp(!0))},[Bn]),oa=(0,T.useCallback)(()=>{Bn&&(cm(),tp(!1))},[Bn]),Nu=(0,T.useCallback)(()=>{Bn?oa():Tp()},[Bn,Tp,oa]),Cs=(0,T.useCallback)((p=bn)=>{let y=p.filter(I=>I.element.isConnected);if(y.length===0){qo([]);return}let b=y[0],L=b.element,N=y.length>1,P=y.map(I=>$t(I.element));if(N){let I={left:Math.min(...P.map(Me=>Me.left)),top:Math.min(...P.map(Me=>Me.top)),right:Math.max(...P.map(Me=>Me.right)),bottom:Math.max(...P.map(Me=>Me.bottom))},V=y.slice(0,5).map(Me=>Me.name).join(", "),ee=y.length>5?` +${y.length-5} more`:"",ge=P.map(Me=>({x:Me.left,y:Me.top+window.scrollY,width:Me.width,height:Me.height})),he=y[y.length-1].element,Ie=P[P.length-1],He=Ie.left+Ie.width/2,de=Ie.top+Ie.height/2,ye=Gm(he);B({id:Date.now().toString(),x:He/window.innerWidth*100,y:ye?de:de+window.scrollY,clientY:de,element:`${y.length} elements: ${V}${ee}`,elementPath:"multi-select",boundingBox:{x:I.left,y:I.top+window.scrollY,width:I.right-I.left,height:I.bottom-I.top},isMultiSelect:!0,isFixed:ye,elementBoundingBoxes:ge,multiSelectElements:y.map(Me=>Me.element),targetElement:he,fullPath:ts(L),accessibility:Ml(L),computedStyles:El(L),computedStylesObj:Cl(L),nearbyElements:Sl(L),cssClasses:es(L),nearbyText:Zi(L),sourceFile:uu(L),attributes:Ki(L,K)})}else{let I=P[0],V=Gm(L);B({id:Date.now().toString(),x:I.left/window.innerWidth*100,y:V?I.top:I.top+window.scrollY,clientY:I.top,element:b.name,elementPath:b.path,boundingBox:{x:I.left,y:V?I.top:I.top+window.scrollY,width:I.width,height:I.height},isFixed:V,fullPath:ts(L),accessibility:Ml(L),computedStyles:El(L),computedStylesObj:Cl(L),nearbyElements:Sl(L),cssClasses:es(L),nearbyText:Zi(L),reactComponents:b.reactComponents,targetElement:L,sourceFile:uu(L),attributes:Ki(L,K)})}qo([]),$e(null)},[bn,K]);(0,T.useEffect)(()=>{O||(B(null),Kt(null),Lr(null),fs([]),$e(null),Pr(!1),qo([]),ys.current=!1,Bn&&oa())},[O,Bn,oa]),(0,T.useEffect)(()=>()=>{cm()},[]),(0,T.useEffect)(()=>{if(!O)return;let p=["p","span","h1","h2","h3","h4","h5","h6","li","td","th","label","blockquote","figcaption","caption","legend","dt","dd","pre","code","em","strong","b","i","u","s","a","time","address","cite","q","abbr","dfn","mark","small","sub","sup","[contenteditable]"].join(", "),y=document.createElement("style");return y.id="agentation-cursor",y.textContent=`
      body { cursor: crosshair !important; }
      body :is(${p}) { cursor: text !important; }
    `,document.head.appendChild(y),()=>{let b=document.getElementById("agentation-cursor");b&&b.remove()}},[O]),(0,T.useEffect)(()=>{if(mp!==null&&O)return document.documentElement.setAttribute("data-drawing-hover",""),()=>document.documentElement.removeAttribute("data-drawing-hover")},[mp,O]),(0,T.useEffect)(()=>{if(!O||M||me||zn||qe)return;let p=null,y=(P,I,V)=>{let ee=Tl(P,I),ge=V?dm(P,I):ee;if(!ge||jt(ge,"[data-feedback-toolbar], [data-annotation-popup], [data-annotation-marker]")){$e(null);return}let{name:fe,elementName:he,path:Ie,reactComponents:He}=cu(ge,Do,K);$e({element:fe,elementName:he,elementPath:Ie,rect:$t(ge),reactComponents:He,isPiercing:V&&ge!==ee}),R({x:P,y:I})},b=P=>{let I=P.composedPath()[0]||P.target;if(jt(I,"[data-feedback-toolbar], [data-annotation-popup], [data-annotation-marker]")){p=null,$e(null);return}p={x:P.clientX,y:P.clientY},y(P.clientX,P.clientY,Cr(P))},L=P=>{(P.key==="Meta"||P.key==="Control")&&p&&y(p.x,p.y,Cr(P))},N=()=>{p=null,$e(null)};return J.addEventListener("mousemove",b),J.addEventListener("keydown",L),J.addEventListener("keyup",L),J.addEventListener("mouseleave",N),window.addEventListener("blur",N),()=>{J.removeEventListener("mousemove",b),J.removeEventListener("keydown",L),J.removeEventListener("keyup",L),J.removeEventListener("mouseleave",N),window.removeEventListener("blur",N)}},[O,M,me,zn,qe,Do,K]);let ra=(0,T.useCallback)((p,y)=>{if(me&&!Br){Zl.current?.shake();return}if(M&&!Hn){if(Ot.current?.querySelector("[data-annotation-popup]:not([data-annotation-card]) textarea")?.value.trim()){Tu.current?.shake();return}vs(!0)}if(Ln.current=y??null,Tr.current=y?.matches(":focus-visible")??!1,ho(!1),fi(!1),Kt(p),Mt(null),Ne(null),lt([]),p.elementBoundingBoxes?.length){let b=[];for(let L of p.elementBoundingBoxes){let N=L.x+L.width/2,P=L.y+L.height/2-window.scrollY,I=Ll(N,P,L);I&&b.push(I)}fs(b),Lr(null)}else if(p.boundingBox){let b=p.boundingBox,L=b.x+b.width/2,N=p.isFixed?b.y+b.height/2:b.y+b.height/2-window.scrollY,P=Ll(L,N,b);if(P){let I=$t(P),V=I.width/b.width,ee=I.height/b.height;V<.5||ee<.5?Lr(null):Lr(P)}else Lr(null);fs([])}else Lr(null),fs([])},[M,Hn,me,Br]);(0,T.useEffect)(()=>{if(!O||zn||qe)return;let p=y=>{if(bs.current){bs.current=!1,y.preventDefault(),y.stopPropagation();return}let b=y.composedPath()[0]||y.target;if(jt(b,"[data-feedback-toolbar]")||jt(b,"[data-annotation-popup]")||jt(b,"[data-annotation-marker]"))return;if(Cr(y)&&!M&&!me){y.preventDefault(),y.stopPropagation(),ys.current=y.shiftKey;let Me=dm(y.clientX,y.clientY);if(!Me)return;let Ke=$t(Me),{name:kt,path:mn,reactComponents:Te}=cu(Me,Do,K),we=bn.findIndex(nt=>nt.element===Me);we>=0?qo(nt=>nt.filter((It,bo)=>bo!==we)):qo(nt=>[...nt,{element:Me,rect:Ke,name:kt,path:mn,reactComponents:Te??void 0}]);return}let L=jt(b,"button, a, input, select, textarea, [role='button'], [onclick]");if(tt.blockInteractions&&(y.preventDefault(),y.stopPropagation()),M&&!Hn){if(L&&!tt.blockInteractions)return;y.preventDefault(),Tu.current?.shake();return}if(me&&!Br){if(L&&!tt.blockInteractions)return;y.preventDefault(),Zl.current?.shake();return}y.preventDefault();let N=Tl(y.clientX,y.clientY);if(!N)return;let{name:P,path:I,reactComponents:V}=cu(N,Do,K),ee=$t(N),ge=y.clientX/window.innerWidth*100,fe=Gm(N),he=fe?y.clientY:y.clientY+window.scrollY,Ie=N.ownerDocument.defaultView?.getSelection(),He;Ie&&Ie.toString().trim().length>0&&(He=Ie.toString().trim().slice(0,500));let de=Cl(N),ye=El(N);vs(!1),B({id:Date.now().toString(),x:ge,y:he,clientY:y.clientY,element:P,elementPath:I,selectedText:He,boundingBox:{x:ee.left,y:fe?ee.top:ee.top+window.scrollY,width:ee.width,height:ee.height},nearbyText:Zi(N),cssClasses:es(N),isFixed:fe,fullPath:ts(N),accessibility:Ml(N),computedStyles:ye,computedStylesObj:de,nearbyElements:Sl(N),reactComponents:V??void 0,sourceFile:uu(N),attributes:Ki(N,K),frame:M0(N,y.clientX,y.clientY),targetElement:N}),$e(null)};return J.addEventListener("click",p,!0),()=>J.removeEventListener("click",p,!0)},[O,zn,qe,M,Hn,me,Br,tt.blockInteractions,Do,K,bn]),(0,T.useEffect)(()=>{if(!O)return;let p=b=>{let L=(b.key==="Meta"||b.key==="Control")&&!Cr(b),N=b.key==="Shift"&&ys.current;(L||N)&&!Wn.current&&bn.length>0&&Cs()},y=()=>{ys.current=!1,qo([]),$e(null),zr.current=null,Wn.current=null,Eu(!1),gi.current?.replaceChildren()};return J.addEventListener("keyup",p),window.addEventListener("blur",y),()=>{J.removeEventListener("keyup",p),window.removeEventListener("blur",y)}},[O,bn,Cs]),(0,T.useEffect)(()=>{if(!O||M||zn||qe)return;let p=y=>{if(y.button!==0)return;bs.current=!1;let b=y.composedPath()[0]||y.target;if(jt(b,"[data-feedback-toolbar]")||jt(b,"[data-annotation-marker]")||jt(b,"[data-annotation-popup]"))return;let L=new Set(["P","SPAN","H1","H2","H3","H4","H5","H6","LI","TD","TH","LABEL","BLOCKQUOTE","FIGCAPTION","CAPTION","LEGEND","DT","DD","PRE","CODE","EM","STRONG","B","I","U","S","A","TIME","ADDRESS","CITE","Q","ABBR","DFN","MARK","SMALL","SUB","SUP"]);!Cr(y)&&(L.has(b.tagName)||b.isContentEditable)||(y.preventDefault(),zr.current={x:y.clientX,y:y.clientY})};return J.addEventListener("mousedown",p),()=>J.removeEventListener("mousedown",p)},[O,M,zn,qe]),(0,T.useEffect)(()=>{if(!O||M)return;let p=y=>{if(!zr.current)return;let b=y.clientX-zr.current.x,L=y.clientY-zr.current.y,N=b*b+L*L,P=Mu*Mu;if(!$o&&N>=P&&(Wn.current=zr.current,Eu(!0),y.preventDefault()),($o||N>=P)&&Wn.current){if(ws.current){let Te=Math.min(Wn.current.x,y.clientX),we=Math.min(Wn.current.y,y.clientY),nt=Math.abs(y.clientX-Wn.current.x),It=Math.abs(y.clientY-Wn.current.y);ws.current.style.transform=`translate(${Te}px, ${we}px)`,ws.current.style.width=`${nt}px`,ws.current.style.height=`${It}px`}let I=Date.now();if(I-Sp.current<u5)return;Sp.current=I;let V=Wn.current.x,ee=Wn.current.y,ge=Math.min(V,y.clientX),fe=Math.min(ee,y.clientY),he=Math.max(V,y.clientX),Ie=Math.max(ee,y.clientY),He=(ge+he)/2,de=(fe+Ie)/2,ye=new Set,Me=[[ge,fe],[he,fe],[ge,Ie],[he,Ie],[He,de],[He,fe],[He,Ie],[ge,de],[he,de]];for(let[Te,we]of Me){let nt=document.elementsFromPoint(Te,we);for(let It of nt)It instanceof HTMLElement&&ye.add(It)}let Ke=J.querySelectorAll("button, a, input, img, p, h1, h2, h3, h4, h5, h6, li, label, td, th, div, span, section, article, aside, nav");for(let Te of Ke)if(Te instanceof HTMLElement){let we=$t(Te),nt=we.left+we.width/2,It=we.top+we.height/2,bo=nt>=ge&&nt<=he&&It>=fe&&It<=Ie,ro=Math.min(we.right,he)-Math.max(we.left,ge),Ts=Math.min(we.bottom,Ie)-Math.max(we.top,fe),ua=ro>0&&Ts>0?ro*Ts:0,Jt=we.width*we.height,Ls=Jt>0?ua/Jt:0;(bo||Ls>.5)&&ye.add(Te)}let kt=[],mn=new Set(["BUTTON","A","INPUT","IMG","P","H1","H2","H3","H4","H5","H6","LI","LABEL","TD","TH","SECTION","ARTICLE","ASIDE","NAV"]);for(let Te of ye){if(jt(Te,"[data-feedback-toolbar]")||jt(Te,"[data-annotation-marker]"))continue;let we=$t(Te);if(!(we.width>window.innerWidth*.8&&we.height>window.innerHeight*.5)&&!(we.width<10||we.height<10)&&we.left<he&&we.right>ge&&we.top<Ie&&we.bottom>fe){let nt=Te.tagName,It=mn.has(nt);if(!It&&(nt==="DIV"||nt==="SPAN")){let bo=Te.textContent&&Te.textContent.trim().length>0,ro=Te.onclick!==null||Te.getAttribute("role")==="button"||Te.getAttribute("role")==="link"||Te.classList.contains("clickable")||Te.hasAttribute("data-clickable");(bo||ro)&&!Te.querySelector("p, h1, h2, h3, h4, h5, h6, button, a")&&(It=!0)}if(It){let bo=!1;for(let ro of kt)if(ro.left<=we.left&&ro.right>=we.right&&ro.top<=we.top&&ro.bottom>=we.bottom){bo=!0;break}bo||kt.push(we)}}}if(gi.current){let Te=gi.current;for(;Te.children.length>kt.length;)Te.removeChild(Te.lastChild);kt.forEach((we,nt)=>{let It=Te.children[nt];It||(It=document.createElement("div"),It.className=z.selectedElementHighlight,Te.appendChild(It)),It.style.transform=`translate(${we.left}px, ${we.top}px)`,It.style.width=`${we.width}px`,It.style.height=`${we.height}px`})}}};return J.addEventListener("mousemove",p,{passive:!0}),()=>J.removeEventListener("mousemove",p)},[O,M,$o,Mu]),(0,T.useEffect)(()=>{if(!O)return;let p=y=>{let b=$o,L=Wn.current;if($o&&L){bs.current=!0;let N=Math.min(L.x,y.clientX),P=Math.min(L.y,y.clientY),I=Math.max(L.x,y.clientX),V=Math.max(L.y,y.clientY),ee=[];J.querySelectorAll("button, a, input, img, p, h1, h2, h3, h4, h5, h6, li, label, td, th").forEach(de=>{if(!(de instanceof HTMLElement)||jt(de,"[data-feedback-toolbar]")||jt(de,"[data-annotation-marker]"))return;let ye=$t(de);ye.width>window.innerWidth*.8&&ye.height>window.innerHeight*.5||ye.width<10||ye.height<10||ye.left<I&&ye.right>N&&ye.top<V&&ye.bottom>P&&ee.push({element:de,rect:ye})});let fe=ee.filter(({element:de})=>!ee.some(({element:ye})=>ye!==de&&de.contains(ye))),he=y.clientX/window.innerWidth*100,Ie=y.clientY+window.scrollY,He=(Cr(y)||bn.length>0)&&!M&&!me;if(fe.length>0)if(He){let de=[...bn];for(let{element:ye,rect:Me}of fe){if(de.some(Te=>Te.element===ye))continue;let{name:Ke,path:kt,reactComponents:mn}=cu(ye,Do,K);de.push({element:ye,rect:Me,name:Ke,path:kt,reactComponents:mn??void 0})}ys.current=y.shiftKey,Cr(y)?qo(de):Cs(de)}else{let de=fe.reduce((Te,{rect:we})=>({left:Math.min(Te.left,we.left),top:Math.min(Te.top,we.top),right:Math.max(Te.right,we.right),bottom:Math.max(Te.bottom,we.bottom)}),{left:1/0,top:1/0,right:-1/0,bottom:-1/0}),ye=fe.slice(0,5).map(({element:Te})=>vr(Te).name).join(", "),Me=fe.length>5?` +${fe.length-5} more`:"",Ke=fe[0].element,kt=Cl(Ke),mn=El(Ke);B({id:Date.now().toString(),x:he,y:Ie,clientY:y.clientY,element:`${fe.length} elements: ${ye}${Me}`,elementPath:"multi-select",boundingBox:{x:de.left,y:de.top+window.scrollY,width:de.right-de.left,height:de.bottom-de.top},isMultiSelect:!0,fullPath:ts(Ke),accessibility:Ml(Ke),computedStyles:mn,computedStylesObj:kt,nearbyElements:Sl(Ke),cssClasses:es(Ke),nearbyText:Zi(Ke),sourceFile:uu(Ke),attributes:Ki(Ke,K)})}else if(He&&!Cr(y))Cs();else if(!He){let de=Math.abs(I-N),ye=Math.abs(V-P);de>20&&ye>20&&B({id:Date.now().toString(),x:he,y:Ie,clientY:y.clientY,element:"Area selection",elementPath:`region at (${Math.round(N)}, ${Math.round(P)})`,boundingBox:{x:N,y:P+window.scrollY,width:de,height:ye},isMultiSelect:!0})}$e(null)}else b&&(bs.current=!0);zr.current=null,Wn.current=null,Eu(!1),gi.current&&(gi.current.innerHTML="")};return J.addEventListener("mouseup",p),()=>J.removeEventListener("mouseup",p)},[O,$o,M,me,Do,K,bn,Cs]);let vo=(0,T.useCallback)(async(p,y,b)=>{let L=tt.webhookUrl||D;if(!L||!tt.webhooksEnabled&&!b)return!1;try{return(await fetch(L,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({event:p,timestamp:Date.now(),url:typeof window<"u"?window.location.href:void 0,...y})})).ok}catch(N){return console.warn("[Agentation] Webhook failed:",N),!1}},[D,tt.webhookUrl,tt.webhooksEnabled]),Iu=(0,T.useRef)(new Map),[Es,m5]=(0,T.useState)(null);(0,T.useEffect)(()=>{let p=M;if(!Z||!p||p.isSubmitted||p.frame||!p.boundingBox||Iu.current.has(p.id))return;let y=(I,V)=>({x:I.x,y:V?I.y:I.y-window.scrollY,width:I.width,height:I.height}),b=y(p.boundingBox,!!p.isFixed),L=p.elementBoundingBoxes?.map(I=>y(I,!1)),N=p.id,P=T1({rect:b,highlights:L}).catch(()=>null);Iu.current.set(N,P),P.then(I=>{!I||!Ge.current||(m5(V=>V&&V.id!==N?V:{id:N,src:I}),vn(V=>V.some(ee=>ee.id===N&&!ee.screenshot)?V.map(ee=>ee.id===N&&!ee.screenshot?{...ee,screenshot:I}:ee):V))})},[M,Z]);let Lp=(0,T.useCallback)(async p=>{if(!Z)return p;let y=new Promise(b=>De(()=>b(null),5e3));return Promise.all(p.map(async b=>{if(b.screenshot)return b;let L=Iu.current.get(b.id);if(!L)return b;let N=await Promise.race([L,y]);return N?{...b,screenshot:N}:b}))},[Z]),p5=(0,T.useCallback)(p=>{if(!M||M.isSubmitted)return;let y={id:M.id,x:M.x,y:M.y,comment:p,element:M.element,elementPath:M.elementPath,timestamp:Date.now(),selectedText:M.selectedText,boundingBox:M.boundingBox,nearbyText:M.nearbyText,cssClasses:M.cssClasses,isMultiSelect:M.isMultiSelect,isFixed:M.isFixed,fullPath:M.fullPath,accessibility:M.accessibility,computedStyles:M.computedStyles,nearbyElements:M.nearbyElements,reactComponents:M.reactComponents,sourceFile:M.sourceFile,attributes:M.attributes,frame:M.frame,elementBoundingBoxes:M.elementBoundingBoxes,screenshot:Es?.id===M.id?Es.src:void 0,...E&&Ht?{sessionId:Ht,url:typeof window<"u"?window.location.href:void 0,status:"pending"}:{}};vn(b=>[...b,y]),B({...M,isSubmitted:!0}),oo.current=y.id,k?.(y),vo("annotation.add",{annotation:y}),vs(!0),window.getSelection()?.removeAllRanges(),E&&Ht&&Le(async()=>{let b=await Ft(E,Ht,y);if(o){let L=Xo(e);qc(e,L.map(N=>N.id===y.id?{...N,id:b.id}:N),Ht)}!Ge.current||xe.current.has(y.id)||b.id!==y.id&&(Kl.current.set(b.id,y.id),oo.current===y.id&&(oo.current=b.id),vn(L=>L.map(N=>N.id===y.id?{...N,id:b.id}:N)),Or.current.delete(y.id)&&Or.current.add(b.id))}).catch(b=>{console.warn("[Agentation] Failed to sync annotation:",b)})},[M,Es,k,vo,E,Ht,Le,e,o]),Du=(0,T.useCallback)(()=>{vs(!0)},[]),$u=(0,T.useCallback)(()=>{B(null),vs(!1)},[]),Au=(0,T.useCallback)(p=>{if(xe.current.has(p))return;xe.current.add(p);let y=Ve.find(b=>b.id===p);me?.id===p&&(ho(!1),fi(!0)),Jl(b=>new Set(b).add(p)),y&&(w?.(y),vo("annotation.delete",{annotation:y})),E&&Le(()=>Dl(E,Ye.current.get(p)??p)).catch(b=>{console.warn("[Agentation] Failed to delete annotation from server:",b)})},[Ve,me,w,vo,E,Le]),ia=(0,T.useCallback)(p=>{if(!p){Mt(null),Ne(null),lt([]);return}if(Mt(p.id),p.elementBoundingBoxes?.length){let y=[];for(let b of p.elementBoundingBoxes){let L=b.x+b.width/2,N=b.y+b.height/2-window.scrollY,P=Ll(L,N,b);P&&y.push(P)}lt(y),Ne(null)}else if(p.boundingBox){let y=p.boundingBox,b=y.x+y.width/2,L=p.isFixed?y.y+y.height/2:y.y+y.height/2-window.scrollY,N=Ll(b,L,y);if(N){let P=$t(N),I=P.width/y.width,V=P.height/y.height;I<.5||V<.5?Ne(null):Ne(N)}else Ne(null);lt([])}else Ne(null),lt([])},[]),f5=(0,T.useCallback)(p=>{if(!me)return;let y={...me,comment:p};Kt(y),vn(b=>b.map(L=>L.id===me.id?y:L)),C?.(y),vo("annotation.update",{annotation:y}),E&&Le(()=>Zc(E,Ye.current.get(me.id)??me.id,{comment:p})).catch(b=>{console.warn("[Agentation] Failed to update annotation on server:",b)}),ho(Tr.current||!!Ln.current?.matches(":hover")),fi(!0)},[me,C,vo,E,Le]),g5=(0,T.useCallback)(()=>{ho(Tr.current||!!Ln.current?.matches(":hover")),fi(!0)},[]),h5=(0,T.useCallback)(()=>{eo&&me&&!M&&Mt(me.id),Kt(null),Lr(null),fs([]),fi(!1)},[eo,me,M]),sa=(0,T.useCallback)((p,y)=>{if(!p.length&&!y)return;it(!0);let b={placements:[..._i.current.placements,...p],rearrange:y??_i.current.rearrange};_i.current=b,cp(b.placements),up(b.rearrange),clearTimeout(je.current),je.current=De(()=>{Io(L=>L.filter(N=>!b.placements.includes(N))),yo(L=>L===b.rearrange?null:L),_i.current={placements:[],rearrange:null},cp([]),up(null),je.current=void 0,Se()},200)},[Se]),Hr=(0,T.useCallback)(()=>{if(!Ge.current)return;let p=new Map(ta.current.map(V=>[V.id,V])),y=[];for(let V of Ve){let ee=p.get(Ye.current.get(V.id)??V.id)??p.get(V.id);ee&&ee.comment===V.comment&&!xe.current.has(ee.id)&&!y.includes(ee)&&y.push(ee)}let b=y.length,L=dp.current,N=Be.filter(V=>L.designPlacements.includes(V)&&!_i.current.placements.includes(V)),P=pe===L.rearrangeState&&pe!==_i.current.rearrange?pe:null,I=no.filter(V=>wu.current.includes(V));if(!(b===0&&I.length===0&&N.length===0&&!P)){for(let V of y)xe.current.add(V.id),Ze.current.add(V.id),_t.current.add(V.id);if(Jl(V=>new Set([...V,...y.map(ee=>ee.id)])),S?.(y),vo("annotations.clear",{annotations:y}),E&&Promise.all(y.map(V=>Le(()=>Dl(E,Ye.current.get(V.id)??V.id)).catch(ee=>{console.warn("[Agentation] Failed to delete annotation from server:",ee)}))),it(!0),i5(V=>V.filter(ee=>!I.includes(ee))),I.length>0&&I.length===wu.current.length){let V=bu.current;V?.getContext("2d")?.clearRect(0,0,V.width,V.height)}sa(N,P),xt===L.blankCanvas&&on===L.wireframePurpose&&Be===L.designPlacements&&pe===L.rearrangeState&&(xt&&rp(!1),on&&Xl(""),Dr.current={rearrange:null,placements:[]},Il(e)),Se()}},[e,Ve,no,Be,pe,xt,on,S,vo,E,Le,Se,sa]),Fu=(0,T.useCallback)(async()=>{let p=ae.start(),y=typeof window<"u"?window.location.pathname+window.location.search+window.location.hash:e,b=qe&&xt,L;if(b){if(Be.length===0&&!pe&&!on)return;L=r?Ol(y,r):""}else{if(L=Am(Ve,y,tt.outputDetail,{appName:r}),!L&&no.length===0&&Be.length===0&&!pe)return;L||(L=Ol(y,r))}if(!b&&no.length>0){let P=new Set;for(let ge of Ve)ge.drawingIndex!=null&&P.add(ge.drawingIndex);let I=bu.current;I&&(I.style.visibility="hidden");let V=[],ee=window.scrollY;for(let ge=0;ge<no.length;ge++){if(P.has(ge))continue;let fe=no[ge];if(fe.points.length<2)continue;let he=fe.fixed?fe.points:fe.points.map(Bt=>({x:Bt.x,y:Bt.y-ee})),Ie=1/0,He=1/0,de=-1/0,ye=-1/0;for(let Bt of he)Ie=Math.min(Ie,Bt.x),He=Math.min(He,Bt.y),de=Math.max(de,Bt.x),ye=Math.max(ye,Bt.y);let Me=de-Ie,Ke=ye-He,kt=Math.hypot(Me,Ke),mn=he[0],Te=he[he.length-1],we=Math.hypot(Te.x-mn.x,Te.y-mn.y),nt,It=we<kt*.35,bo=Me/Math.max(Ke,1);if(It&&kt>20){let Bt=Math.max(Me,Ke)*.15,Jo=0;for(let Wr of he){let x5=Wr.x-Ie<Bt,v5=de-Wr.x<Bt,w5=Wr.y-He<Bt,b5=ye-Wr.y<Bt;(x5||v5)&&(w5||b5)&&Jo++}nt=Jo>he.length*.15?"box":"circle"}else bo>3&&Ke<40?nt="underline":we>kt*.5?nt="arrow":nt="drawing";let ro=Math.min(10,he.length),Ts=Math.max(1,Math.floor(he.length/ro)),ua=new Set,Jt=[],Ls=[mn];for(let Bt=Ts;Bt<he.length-1;Bt+=Ts)Ls.push(he[Bt]);Ls.push(Te);for(let Bt of Ls){let Jo=Tl(Bt.x,Bt.y);if(!Jo||ua.has(Jo)||jt(Jo,"[data-feedback-toolbar]"))continue;ua.add(Jo);let{name:Wr}=vr(Jo);Jt.includes(Wr)||Jt.push(Wr)}let da=`${Math.round(Ie)},${Math.round(He)} \u2192 ${Math.round(de)},${Math.round(ye)}`,yi;(nt==="circle"||nt==="box")&&Jt.length>0?yi=`${nt==="box"?"Boxed":"Circled"} **${Jt[0]}**${Jt.length>1?` (and ${Jt.slice(1).join(", ")})`:""} (region: ${da})`:nt==="underline"&&Jt.length>0?yi=`Underlined **${Jt[0]}** (${da})`:nt==="arrow"&&Jt.length>=2?yi=`Arrow from **${Jt[0]}** to **${Jt[Jt.length-1]}** (${Math.round(mn.x)},${Math.round(mn.y)} \u2192 ${Math.round(Te.x)},${Math.round(Te.y)})`:Jt.length>0?yi=`${nt==="arrow"?"Arrow":"Drawing"} near **${Jt.join("**, **")}** (region: ${da})`:yi=`Drawing at ${da}`,V.push(yi)}I&&(I.style.visibility=""),V.length>0&&(L+=`
**Drawings:**
`,V.forEach((ge,fe)=>{L+=`${fe+1}. ${ge}
`}))}if((Be.length>0||b&&on)&&(L+=`
`+km(Be,{width:window.innerWidth,height:window.innerHeight},{blankCanvas:xt,wireframePurpose:on||void 0},tt.outputDetail)),pe){let P=Sm(pe,tt.outputDetail,{width:window.innerWidth,height:window.innerHeight});P&&(L+=`
`+P)}if(L=Wm(Ve,L,l),!L){re(!1);return}let N=!h||await L1(L);g?.(L),ae.isCurrent(p)&&(re(N),N&&(ae.schedule(p,()=>re(!1),2e3),tt.autoClearAfterCopy&&ae.schedule(p,Hr,500)))},[Ve,no,Be,pe,xt,qe,lp,on,e,tt.outputDetail,Do,K,tt.autoClearAfterCopy,Hr,ae,h,l,r,g]),Ou=V1(tt.webhookUrl)||V1(D||""),wo=x!=null||Ou&&!tt.webhooksEnabled,la=O?wo?337:297:44,Bu=(0,T.useCallback)(async()=>{let p=Y.start(),y=typeof window<"u"?window.location.href:e,b=typeof window<"u"?window.location.pathname+window.location.search+window.location.hash:e;if(Ve.length===0&&Be.length===0&&!pe)return;ve("sending");let L=await Lp(Ve);if(!Y.isCurrent(p))return;let N=Am(L,b,tt.outputDetail,{appName:r,screenshotReference:q});if(N||(N=Ol(b,r)),Be.length>0&&(N+=`
`+km(Be,{width:window.innerWidth,height:window.innerHeight},{blankCanvas:xt,wireframePurpose:on||void 0},tt.outputDetail)),pe){let ee=Sm(pe,tt.outputDetail,{width:window.innerWidth,height:window.innerHeight});ee&&(N+=`
`+ee)}let P=!0;try{await x?.(N,L)}catch(ee){console.warn("[Agentation] Submit callback failed:",ee),P=!1}if(!Y.isCurrent(p))return;let I=Ou?await vo("submit",{output:N,annotations:L,url:y},!0):!0,V=P&&I&&wo;Y.isCurrent(p)&&(ve(V?"sent":"failed"),Y.schedule(p,()=>ve("idle"),2500),V&&tt.autoClearAfterCopy&&Y.schedule(p,Hr,500))},[x,r,vo,Ve,Be,pe,xt,lp,e,tt.outputDetail,Do,K,tt.autoClearAfterCopy,Hr,Ou,wo,Y,Lp,q]);(0,T.useEffect)(()=>{let y=(P=!1)=>{xs.current?.dragging&&(ql.current=P,bp(!1)),xs.current=null},b=P=>{let I=xs.current;if(!I)return;if((P.buttons&1)===0){y();return}let V=P.clientX-I.x,ee=P.clientY-I.y,ge=Math.sqrt(V*V+ee*ee);if(!I.dragging&&ge>10&&(I.dragging=!0,bp(!0)),I.dragging){let fe=I.toolbarX+V,he=I.toolbarY+ee,Ie=20,He=337,de=44,Me=He-la,Ke=Ie-Me,kt=window.innerWidth-Ie-He;fe=Math.max(Ke,Math.min(kt,fe)),he=Math.max(Ie,Math.min(window.innerHeight-de-Ie,he)),Cu({x:fe,y:he})}},L=()=>y(!0),N=()=>y();return J.addEventListener("mousemove",b),J.addEventListener("mouseup",L,!0),window.addEventListener("blur",N),()=>{J.removeEventListener("mousemove",b),J.removeEventListener("mouseup",L,!0),window.removeEventListener("blur",N)}},[la]);let y5=(0,T.useCallback)(p=>{if(ql.current=!1,xs.current=null,p.button!==0||p.target.closest("button")&&(p.target.closest("button")!==Zn.current||O)||p.target.closest("[data-agentation-settings-panel]"))return;let y=p.currentTarget.parentElement;if(!y)return;let b=$t(y);xs.current={x:p.clientX,y:p.clientY,toolbarX:b.left,toolbarY:b.top,dragging:!1}},[O]);(0,T.useLayoutEffect)(()=>{if(!Nt)return;let p=()=>{let N=Nt.x,P=Nt.y,ee=20-(337-la),ge=window.innerWidth-20-337;N=Math.max(ee,Math.min(ge,N)),P=Math.max(20,Math.min(window.innerHeight-44-20,P)),(N!==Nt.x||P!==Nt.y)&&Cu({x:N,y:P})};return p(),window.addEventListener("resize",p),()=>window.removeEventListener("resize",p)},[Nt,la]),(0,T.useEffect)(()=>{if(!i)return;let p=b=>{if(b.defaultPrevented||b.isComposing||b.altKey)return;let L=b.composedPath()[0]||b.target,N=L.tagName==="INPUT"||L.tagName==="TEXTAREA"||L.tagName==="SELECT"||L.isContentEditable;if(b.key==="Escape"){if(c&&!M&&!me&&(O||nn||qe||zn||bn.length)&&(b.preventDefault(),b.stopPropagation()),nn){b.preventDefault(),Pr(!1),Mr.current?.focus();return}if(qe){ci?Ir(null):Ss();return}if(zn){vu(!1);return}if(bn.length>0){qo([]);return}M||me||O&&(rn(),na())}if((b.metaKey||b.ctrlKey)&&b.shiftKey&&(b.key==="f"||b.key==="F")){b.preventDefault(),rn(),O?na():(Zn.current?.blur(),G.current=!0,Ae(!0));return}!O||N||b.metaKey||b.ctrlKey||b.repeat||((b.key==="p"||b.key==="P")&&(b.preventDefault(),rn(),Nu()),(b.key==="l"||b.key==="L")&&(b.preventDefault(),rn(),zn&&vu(!1),nn&&Pr(!1),M&&Du(),qe?Ss():Pu()),(b.key==="h"||b.key==="H")&&Ve.length>0&&(b.preventDefault(),rn(),fo(P=>!P)),(b.key==="c"||b.key==="C")&&(Ve.length>0||Be.length>0||pe)&&(b.preventDefault(),rn(),Fu()),(b.key==="x"||b.key==="X")&&(Ve.length>0||Be.length>0||pe)&&(b.preventDefault(),rn(),Hr(),Be.length>0&&Io([]),pe&&yo(null)),(b.key==="s"||b.key==="S")&&Ve.length>0&&wo&&ie==="idle"&&(b.preventDefault(),rn(),Bu()))},y=!!c;return J.addEventListener("keydown",p,y),()=>J.removeEventListener("keydown",p,y)},[i,c,me,O,zn,qe,ci,Be,pe,M,Ve.length,wo,ie,Bu,Nu,Fu,Hr,bn,nn,na,Pu,Ss]);let Ms=Ve.length>0,aa=T0(),ca=Ve.filter(p=>p.kind!=="placement"&&p.kind!=="rearrange"),Rp=ca.flatMap((p,y)=>{let b=aa(p);return b?[{annotation:b,index:y}]:[]}),Pp=M&&!M.isSubmitted?aa({...M,comment:"",timestamp:0}):null,Np=[...Pe?Rp.map(p=>({...p,pending:!1})):[],...Pp?[{annotation:Pp,index:ca.length,pending:!0}]:[]];(0,T.useEffect)(()=>{let p=new Set(Pe&&!Qo?Rp.map(({annotation:y})=>y.id):[]);oo.current&&!p.has(oo.current)&&(oo.current=null);for(let y of Ko)p.has(y)||Lu(y)}),(0,T.useEffect)(()=>{me&&Ko.has(me.id)&&(ho(!1),fi(!0))},[me,Ko]);let Ip=(0,T.useCallback)(p=>{!dt&&p.id!==oo.current&&ia(p)},[dt,ia]),Dp=(0,T.useCallback)(p=>{ut===p&&ia(null)},[ut,ia]),$p=(0,T.useCallback)((p,y)=>{if(me&&!Br){Zl.current?.shake();return}Hn&&$u(),tt.markerClickBehavior==="delete"?Au(p.id):ra(p,y)},[tt.markerClickBehavior,Au,ra,Hn,$u,me,Br]),Rn=me??(ea&&!M&&!Oe?Ve.find(p=>p.id===ut&&!Ko.has(p.id)):null),Ap=Bn?"Resume animations":"Pause animations",Fp=qe?"Exit layout mode":"Layout mode",Op=po?"Hide markers":"Show markers",Bp=l!=="markdown"&&!Wm(Ve,"",l),zp=typeof l=="object"?`Copy ${l.attribute}`:l==="source"?"Copy source paths":l==="classes"?"Copy classes":qe&&xt?"Copy layout":"Copy feedback",hi=O?0:-1;return!at||Qo?null:(0,X.jsxs)(Y0,{host:"agentation-toolbar",className:j,children:[(0,X.jsxs)("style",{"data-agentation-styles":"toolbar",children:[vk,bk]}),(0,X.jsxs)("div",{ref:Ot,className:z.positionContext,style:{display:"contents"},"data-agentation-theme":xo?"dark":"light","data-agentation-accent":tt.annotationColorId,"data-agentation-root":"",children:[(0,X.jsx)("div",{className:z.toolbar,"data-feedback-toolbar":!0,"data-agentation-toolbar":!0,"data-dragging":Gl||void 0,style:Nt?{left:Nt.x,top:Nt.y,right:"auto",bottom:"auto"}:void 0,children:(0,X.jsxs)("div",{className:`${z.toolbarContainer} ${O?z.expanded:z.collapsed} ${yp?z.entrance:""} ${On?z.hiding:""} ${wo?z.serverConnected:""}`,onMouseDown:y5,children:[(0,X.jsxs)("div",{className:`${z.controlsContent} ${O?z.visible:z.hidden} ${Nt&&Nt.y<100?z.tooltipBelow:""} ${op||nn?z.tooltipsHidden:""} ${ku?z.tooltipsInSession:""}`,ref:p=>{go.current=p,p?.toggleAttribute("inert",!O)},role:"group","aria-label":"Feedback controls","aria-hidden":!O,onMouseEnter:fp,onMouseLeave:gp,children:[(0,X.jsxs)("div",{className:`${z.buttonWrapper} ${Nt&&Nt.x<120?z.buttonWrapperAlignLeft:""}`,children:[(0,X.jsx)("button",{className:z.controlButton,onClick:p=>{p.stopPropagation(),rn(),Nu()},"data-active":Bn,"aria-label":Ap,"aria-pressed":Bn,tabIndex:hi,children:(0,X.jsx)(ey,{size:24,isPaused:Bn})}),(0,X.jsxs)("span",{className:z.buttonTooltip,children:[Ap,i&&(0,X.jsx)("span",{className:z.shortcut,children:"P"})]})]}),(0,X.jsxs)("div",{className:z.buttonWrapper,children:[(0,X.jsx)("button",{className:`${z.controlButton} ${xo?"":z.light}`,onClick:p=>{p.stopPropagation(),rn(),zn&&vu(!1),nn&&Pr(!1),M&&Du(),qe?Ss():Pu()},"data-active":qe,"aria-label":Fp,"aria-pressed":qe,tabIndex:hi,style:qe&&xt?{color:"#f97316",background:"rgba(249, 115, 22, 0.25)"}:void 0,children:(0,X.jsx)(ay,{size:21})}),(0,X.jsxs)("span",{className:z.buttonTooltip,children:[Fp,i&&(0,X.jsx)("span",{className:z.shortcut,children:"L"})]})]}),(0,X.jsxs)("div",{className:z.buttonWrapper,children:[(0,X.jsx)("button",{className:z.controlButton,onClick:p=>{p.stopPropagation(),rn(),fo(!po)},disabled:!Ms||qe,"aria-label":Op,tabIndex:hi,children:(0,X.jsx)(Z0,{size:24,isOpen:po})}),(0,X.jsxs)("span",{className:z.buttonTooltip,children:[Op,i&&(0,X.jsx)("span",{className:z.shortcut,children:"H"})]})]}),(0,X.jsxs)("div",{className:z.buttonWrapper,children:[(0,X.jsx)("button",{className:`${z.controlButton} ${U?z.statusShowing:""}`,onClick:p=>{p.stopPropagation(),rn(),Fu()},disabled:Bp||(qe&&xt?Be.length===0&&!pe?.sections?.length:!Ms&&no.length===0&&Be.length===0&&!pe?.sections?.length),"data-active":U,"aria-label":zp,tabIndex:hi,children:(0,X.jsx)(K0,{size:24,copied:U,tint:qe&&xt&&(Be.length>0||pe?.sections?.length)?"#f97316":void 0})}),(0,X.jsxs)("span",{className:z.buttonTooltip,children:[Bp?"No matching metadata":zp,i&&(0,X.jsx)("span",{className:z.shortcut,children:"C"})]})]}),(0,X.jsxs)("div",{className:`${z.buttonWrapper} ${z.sendButtonWrapper} ${O&&wo?z.sendButtonVisible:""}`,children:[(0,X.jsxs)("button",{className:`${z.controlButton} ${ie==="sent"||ie==="failed"?z.statusShowing:""}`,onClick:p=>{p.stopPropagation(),rn(),Bu()},disabled:!Ms||!wo||ie==="sending","data-no-hover":ie==="sent"||ie==="failed",tabIndex:O&&wo?0:-1,"aria-label":"Send Annotations","aria-hidden":!wo,children:[(0,X.jsx)(J0,{size:24,state:ie}),Ms&&ie==="idle"&&(0,X.jsx)("span",{className:z.buttonBadge,children:Ve.length})]}),(0,X.jsxs)("span",{className:z.buttonTooltip,children:["Send Annotations",i&&(0,X.jsx)("span",{className:z.shortcut,children:"S"})]})]}),(0,X.jsxs)("div",{className:z.buttonWrapper,children:[(0,X.jsx)("button",{className:z.controlButton,onClick:p=>{p.stopPropagation(),rn(),Hr()},disabled:!Ms&&no.length===0&&Be.length===0&&!pe?.sections?.length,"data-danger":!0,"aria-label":"Clear all",tabIndex:hi,children:(0,X.jsx)(ny,{size:24})}),(0,X.jsxs)("span",{className:z.buttonTooltip,children:["Clear all",i&&(0,X.jsx)("span",{className:z.shortcut,children:"X"})]})]}),(0,X.jsxs)("div",{className:z.buttonWrapper,children:[(0,X.jsx)("button",{ref:Mr,"aria-label":"Settings","aria-expanded":nn,tabIndex:hi,className:z.controlButton,onClick:p=>{p.stopPropagation(),rn(),qe&&Ss(),Ee.current=!nn&&p.detail===0,Pr(!nn)},children:(0,X.jsx)(ty,{size:24})}),E&&Ar!=="disconnected"&&(0,X.jsx)("span",{className:`${z.mcpIndicator} ${z[Ar]} ${nn?z.hidden:""}`,title:Ar==="connected"?"MCP Connected":"MCP Connecting..."}),(0,X.jsx)("span",{className:z.buttonTooltip,children:"Settings"})]}),(0,X.jsx)("div",{className:z.divider}),(0,X.jsx)("div",{className:z.togglePlaceholder,"aria-hidden":"true"})]}),(0,X.jsxs)("div",{className:`${z.buttonWrapper} ${z.toggleWrapper} ${Nt&&Nt.y<100?z.tooltipBelow:""} ${!O||op||nn?z.tooltipsHidden:""} ${ku?z.tooltipsInSession:""} ${Nt&&typeof window<"u"&&Nt.x>window.innerWidth-120?z.buttonWrapperAlignRight:""}`,onMouseEnter:fp,onMouseLeave:gp,children:[(0,X.jsx)("button",{ref:Zn,type:"button",className:`${z.toggleContent} ${O?z.expandedToggle:""}`,"aria-label":O?"Exit":"Start feedback mode","aria-expanded":O,"aria-keyshortcuts":i?"Meta+Shift+F Control+Shift+F":void 0,title:O?void 0:i?"Start feedback mode (\u2318\u21E7F / Ctrl+Shift+F)":"Start feedback mode",onClick:p=>{if(ql.current){ql.current=!1,p.preventDefault();return}p.stopPropagation(),O?(rn(),na()):(p.currentTarget.blur(),G.current=p.detail===0,Ae(!0))},children:(0,X.jsxs)("span",{className:z.toggleIcon,children:[(0,X.jsx)(my,{active:O}),ca.length>0&&(0,X.jsx)("span",{className:`${z.badge} ${O?z.fadeOut:""} ${yp?z.entrance:""}`,children:ca.length})]})}),(0,X.jsxs)("span",{className:z.buttonTooltip,"aria-hidden":!O,children:["Exit",i&&(0,X.jsx)("span",{className:z.shortcut,children:"Esc"})]})]}),(0,X.jsx)(xy,{visible:qe&&O,activeType:ci,onSelect:p=>{Ir(ci===p?null:p)},isDarkMode:xo,sectionCount:pe?.sections.length??0,onDetectSections:()=>{let p=Sy(),y=pe?.sections??[],b=new Set(y.map(I=>I.selector)),L=p.filter(I=>!b.has(I.selector)),N=[...y,...L],P=[...pe?.originalOrder??[],...L.map(I=>I.id)];yo({sections:N,originalOrder:P,detectedAt:Date.now()})},placementCount:Be.length,onClearPlacements:()=>{sa(Be,pe)},blankCanvas:xt,onBlankCanvasChange:p=>{let y={sections:[],originalOrder:[],detectedAt:Date.now()};p?(gu.current={rearrange:pe,placements:Be},yo(Dr.current.rearrange||y),Io(Dr.current.placements),Ir(null)):(Dr.current={rearrange:pe,placements:Be},yo(gu.current.rearrange||y),Io(gu.current.placements)),rp(p)},wireframePurpose:on,onWireframePurposeChange:Xl,Tooltip:Ro,onDragStart:(p,y)=>{y.preventDefault();let b=te[p],L=null,N=!1,P=y.clientX,I=y.clientY,ee=y.target.closest("[data-feedback-toolbar]")?.getBoundingClientRect().top??window.innerHeight,ge=he=>{let Ie=he.clientX-P,He=he.clientY-I;if(!N&&(Math.abs(Ie)>4||Math.abs(He)>4)&&(N=!0,L=document.createElement("div"),L.className=`${$.dragPreview}${xt?` ${$.dragPreviewWireframe}`:""}`,Ot.current?.appendChild(L)),!L)return;let de=Math.max(0,ee-he.clientY),ye=Math.min(1,de/180),Me=1-Math.pow(1-ye,2),Ke=28,kt=20,mn=Math.min(140,b.width*.18),Te=Math.min(90,b.height*.18),we=Ke+(mn-Ke)*Me,nt=kt+(Te-kt)*Me;L.style.width=`${we}px`,L.style.height=`${nt}px`,L.style.left=`${he.clientX-we/2}px`,L.style.top=`${he.clientY-nt/2}px`,L.style.opacity=`${.5+.5*Me}`,L.textContent=Me>.25?p:""},fe=he=>{if(window.removeEventListener("mousemove",ge),window.removeEventListener("mouseup",fe),L&&L.remove(),N){let Ie=b.width,He=b.height,de=window.scrollY,ye=Math.max(0,he.clientX-Ie/2),Me=Math.max(0,he.clientY+de-He/2),Ke={id:`dp-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,type:p,x:ye,y:Me,width:Ie,height:He,scrollY:de,timestamp:Date.now()};Io(kt=>[...kt,Ke]),Ir(null),hs.current=new Set,ap(kt=>kt+1)}};window.addEventListener("mousemove",ge),window.addEventListener("mouseup",fe)}}),(0,X.jsx)(W1,{settings:tt,onSettingsChange:a5,isDarkMode:xo,onToggleTheme:c5,isDevMode:vp,connectionStatus:Ar,endpoint:E,onExited:d5,isOpen:O&&nn,toolbarNearBottom:!!Nt&&Nt.y<230,settingsPage:J1,onSettingsPageChange:np,onHideToolbar:_5})]})}),(qe||Nr)&&(0,X.jsx)("div",{className:`${$.blankCanvas} ${ip?$.visible:""} ${e5?$.gridActive:""}`,style:{"--canvas-opacity":sp},"data-feedback-toolbar":!0}),qe&&xt&&ip&&(0,X.jsxs)("div",{className:$.wireframeNotice,"data-feedback-toolbar":!0,children:[(0,X.jsxs)("div",{className:$.wireframeOpacityRow,children:[(0,X.jsx)("span",{className:$.wireframeOpacityLabel,children:"Toggle Opacity"}),(0,X.jsx)("input",{type:"range",className:$.wireframeOpacitySlider,min:0,max:1,step:.01,value:sp,onChange:p=>Z1(Number(p.target.value))})]}),(0,X.jsxs)("div",{className:$.wireframeNoticeTitleRow,children:[(0,X.jsx)("span",{className:$.wireframeNoticeTitle,children:"Wireframe Mode"}),(0,X.jsx)("span",{className:$.wireframeNoticeDivider}),(0,X.jsx)("button",{className:$.wireframeStartOver,onClick:()=>{sa(Be,pe),Dr.current={rearrange:null,placements:[]},Xl(""),Il(e)},children:"Start Over"})]}),"Drag components onto the canvas.",(0,X.jsx)("br",{}),"Copied output will only include the wireframed layout."]}),(qe||Nr)&&(0,X.jsx)(yy,{placements:Be,onChange:Io,activeComponent:Nr?null:ci,onActiveComponentChange:Ir,isDarkMode:xo,exiting:Nr,onInteractionChange:t5,passthrough:!ci,extraSnapRects:pe?.sections.map(p=>p.currentRect),deselectSignal:n5,clearingPlacements:ui,wireframe:xt,onSelectionChange:(p,y)=>{hs.current=p,y||(Ql.current=new Set,r5(b=>b+1))},onDragMove:(p,y)=>{let b=Ql.current;if(!(!b.size||!pe)){if(!to.current){to.current=new Map;for(let L of pe.sections)b.has(L.id)&&to.current.set(L.id,{x:L.currentRect.x,y:L.currentRect.y})}for(let L of pe.sections){if(!b.has(L.id)||!to.current.get(L.id))continue;let P=Ot.current?.querySelector(`[data-rearrange-section="${L.id}"]`);P&&(P.style.transform=`translate(${p}px, ${y}px)`)}}},onDragEnd:(p,y,b)=>{let L=Ql.current,N=to.current;if(to.current=null,!(!L.size||!pe||!N)){for(let P of L){let I=Ot.current?.querySelector(`[data-rearrange-section="${P}"]`);I&&(I.style.transform="")}b&&yo(P=>P&&{...P,sections:P.sections.map(I=>{let V=N.get(I.id);return V?{...I,currentRect:{...I.currentRect,x:Math.max(0,V.x+p),y:Math.max(0,V.y+y)}}:I})})}}}),(qe||Nr)&&pe&&(0,X.jsx)(Py,{rearrangeState:pe,onChange:yo,isDarkMode:xo,exiting:Nr,blankCanvas:xt,extraSnapRects:Be.map(p=>({x:p.x,y:p.y,width:p.width,height:p.height})),clearing:pe===di,deselectSignal:o5,onSelectionChange:(p,y)=>{Ql.current=p,y||(hs.current=new Set,ap(b=>b+1))},onDragMove:(p,y)=>{let b=hs.current;if(b.size){if(!to.current){to.current=new Map;for(let L of Be)b.has(L.id)&&to.current.set(L.id,{x:L.x,y:L.y})}for(let L of b){let N=Ot.current?.querySelector(`[data-design-placement="${L}"]`);N&&(N.style.transform=`translate(${p}px, ${y}px)`)}}},onDragEnd:(p,y,b)=>{let L=hs.current,N=to.current;if(to.current=null,!(!L.size||!N)){for(let P of L){let I=Ot.current?.querySelector(`[data-design-placement="${P}"]`);I&&(I.style.transform="")}b&&Io(P=>P.map(I=>{let V=N.get(I.id);return V?{...I,x:Math.max(0,V.x+p),y:Math.max(0,V.y+y)}:I}))}}}),(0,X.jsx)("canvas",{ref:bu,className:`${z.drawCanvas} ${zn?z.active:""}`,"aria-hidden":"true",style:{opacity:ea?1:0,transition:"opacity 0.15s ease"},"data-feedback-toolbar":!0}),(0,X.jsx)("div",{className:z.markersLayer,"data-feedback-toolbar":!0,children:Np.filter(({annotation:p})=>!p.isFixed).map(({annotation:p,index:y,pending:b},L,N)=>(0,X.jsx)(Ym,{annotation:p,pending:b,globalIndex:y,layerIndex:L,layerSize:N.length,isExiting:b?Hn:dt,isClearing:Ze.current.has(p.id),isAnimated:Or.current.has(p.id),isNew:oo.current===p.id,onEnterComplete:kp,isHovered:!dt&&ut===p.id,isRemoving:Ko.has(p.id),onRemoveComplete:Lu,isEditingAny:!!me,renumberFrom:yt,markerClickBehavior:tt.markerClickBehavior,onHoverEnter:Ip,onHoverLeave:Dp,onClick:$p,onContextMenu:ra},Kl.current.get(p.id)??p.id))}),(0,X.jsx)("div",{className:z.fixedMarkersLayer,"data-feedback-toolbar":!0,children:Np.filter(({annotation:p})=>p.isFixed).map(({annotation:p,index:y,pending:b},L,N)=>(0,X.jsx)(Ym,{annotation:p,pending:b,globalIndex:y,layerIndex:L,layerSize:N.length,isExiting:b?Hn:dt,isClearing:Ze.current.has(p.id),isAnimated:Or.current.has(p.id),isNew:oo.current===p.id,onEnterComplete:kp,isHovered:!dt&&ut===p.id,isRemoving:Ko.has(p.id),onRemoveComplete:Lu,isEditingAny:!!me,renumberFrom:yt,markerClickBehavior:tt.markerClickBehavior,onHoverEnter:Ip,onHoverLeave:Dp,onClick:$p,onContextMenu:ra},Kl.current.get(p.id)??p.id))}),O&&Fe&&!M&&!me&&!Zm&&!$o&&(0,X.jsx)(U1,{x:Et.x,y:Et.y,elementName:Fe.elementName,reactComponents:Fe.reactComponents}),O&&(0,X.jsxs)("div",{className:z.overlay,"data-feedback-toolbar":!0,style:M||me?{zIndex:"inherit"}:void 0,children:[Fe?.rect&&!M&&!Zm&&!$o&&(0,X.jsx)("div",{className:`${z.hoverHighlight} ${z.enter}`,style:{left:Fe.rect.left,top:Fe.rect.top,width:Fe.rect.width,height:Fe.rect.height,borderColor:"color-mix(in srgb, var(--agentation-color-accent) 50%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 4%, transparent)",...Fe.isPiercing?{borderStyle:"dashed"}:{}}}),bn.filter(p=>p.element.isConnected).map((p,y)=>{let b=$t(p.element),L=bn.length>1;return(0,X.jsx)("div",{className:L?z.multiSelectOutline:z.singleSelectOutline,style:{position:"fixed",left:b.left,top:b.top,width:b.width,height:b.height,...L?{}:{borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}}},y)}),ut&&!M&&(()=>{let p=Ve.find(N=>N.id===ut);if(!p?.boundingBox)return null;if(p.elementBoundingBoxes?.length)return et.length>0?et.filter(N=>N.isConnected).map((N,P)=>{let I=$t(N);return(0,X.jsx)("div",{className:`${z.multiSelectOutline} ${z.enter}`,style:{left:I.left,top:I.top,width:I.width,height:I.height}},`hover-outline-live-${P}`)}):p.elementBoundingBoxes.map((N,P)=>(0,X.jsx)("div",{className:`${z.multiSelectOutline} ${z.enter}`,style:{left:N.x,top:N.y-Rr,width:N.width,height:N.height}},`hover-outline-${P}`));let y=st&&st.isConnected?$t(st):null,b=y?{x:y.left,y:y.top,width:y.width,height:y.height}:{x:p.boundingBox.x,y:p.isFixed?p.boundingBox.y:p.boundingBox.y-Rr,width:p.boundingBox.width,height:p.boundingBox.height},L=p.isMultiSelect;return(0,X.jsx)("div",{className:`${L?z.multiSelectOutline:z.singleSelectOutline} ${z.enter}`,style:{left:b.x,top:b.y,width:b.width,height:b.height,...L?{}:{borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}}})})(),M&&(0,X.jsxs)(X.Fragment,{children:[M.multiSelectElements?.length?M.multiSelectElements.filter(p=>p.isConnected).map((p,y)=>{let b=$t(p);return(0,X.jsx)("div",{className:`${z.multiSelectOutline} ${Hn?z.exit:z.enter}`,style:{left:b.left,top:b.top,width:b.width,height:b.height}},`pending-multi-${y}`)}):M.targetElement&&M.targetElement.isConnected?(()=>{let p=$t(M.targetElement);return(0,X.jsx)("div",{className:`${z.singleSelectOutline} ${Hn?z.exit:z.enter}`,style:{left:p.left,top:p.top,width:p.width,height:p.height,borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}})})():M.boundingBox&&(0,X.jsx)("div",{className:`${M.isMultiSelect?z.multiSelectOutline:z.singleSelectOutline} ${Hn?z.exit:z.enter}`,style:{left:M.boundingBox.x,top:M.boundingBox.y-Rr,width:M.boundingBox.width,height:M.boundingBox.height,...M.isMultiSelect?{}:{borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}}}),(()=>{let p=aa(M)??M,y=p.x,b=p.isFixed?p.y:p.y-Rr;return(0,X.jsx)(X.Fragment,{children:(0,X.jsx)(is,{ref:Tu,element:M.element,selectedText:M.selectedText,screenshot:Es?.id===M.id?Es.src:void 0,allowEmpty:typeof l=="object"&&!!M.attributes?.[l.attribute],onOpenSource:a&&M.sourceFile?()=>a(M.sourceFile):void 0,computedStyles:M.computedStylesObj,placeholder:typeof l=="object"&&M.attributes?.[l.attribute]?"Add a note (optional)":M.element==="Area selection"?"What should change in this area?":M.isMultiSelect?"Feedback for this group of elements...":"What should change?",onSubmit:p5,onExitComplete:$u,onCancel:Du,isExiting:Hn,lightMode:!xo,accentColor:M.isMultiSelect?"var(--agentation-color-green)":"var(--agentation-color-accent)",style:{left:Math.max(160,Math.min(window.innerWidth-160,y/100*window.innerWidth)),...b>window.innerHeight-290?{bottom:window.innerHeight-b+20}:{top:b+20}}},M.id)})})()]}),me&&(0,X.jsx)(X.Fragment,{children:me.elementBoundingBoxes?.length?Km.length>0?Km.filter(p=>p.isConnected).map((p,y)=>{let b=$t(p);return(0,X.jsx)("div",{className:`${z.multiSelectOutline} ${z.enter}`,style:{left:b.left,top:b.top,width:b.width,height:b.height}},`edit-multi-live-${y}`)}):me.elementBoundingBoxes.map((p,y)=>(0,X.jsx)("div",{className:`${z.multiSelectOutline} ${z.enter}`,style:{left:p.x,top:p.y-Rr,width:p.width,height:p.height}},`edit-multi-${y}`)):(()=>{let p=ai&&ai.isConnected?$t(ai):null,y=p?{x:p.left,y:p.top,width:p.width,height:p.height}:me.boundingBox?{x:me.boundingBox.x,y:me.isFixed?me.boundingBox.y:me.boundingBox.y-Rr,width:me.boundingBox.width,height:me.boundingBox.height}:null;return y?(0,X.jsx)("div",{className:`${me.isMultiSelect?z.multiSelectOutline:z.singleSelectOutline} ${z.enter}`,style:{left:y.x,top:y.y,width:y.width,height:y.height,...me.isMultiSelect?{}:{borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}}}):null})()}),$o&&(0,X.jsxs)(X.Fragment,{children:[(0,X.jsx)("div",{ref:ws,className:z.dragSelection}),(0,X.jsx)("div",{ref:gi,className:z.highlightsContainer})]})]}),(0,X.jsx)(D1,{ref:Zl,annotation:Rn?aa(Rn)??me:null,editing:!!me,exiting:Br,restorePreview:eo,scrollY:Rr,lightMode:!xo,onExited:h5,editorProps:Rn?{element:Rn.element,selectedText:Rn.selectedText,screenshot:Rn.screenshot,allowEmpty:typeof l=="object"&&!!Rn.attributes?.[l.attribute],onOpenSource:a&&Rn.sourceFile?()=>a(Rn.sourceFile):void 0,computedStyles:I0(Rn.computedStyles),placeholder:"Edit your feedback...",initialValue:Rn.comment,submitLabel:"Save",onSubmit:f5,onCancel:g5,onDelete:()=>Au(Rn.id),accentColor:Rn.isMultiSelect?"var(--agentation-color-green)":"var(--agentation-color-accent)"}:void 0})]})]})}var Sk="3.1.2";return L5(Ck);})();
