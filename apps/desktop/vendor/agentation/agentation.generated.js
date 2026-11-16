/*! Agentation 3.1.2 (fork maddada/agentation 7cc690684d07a293a5175b1ac49db2e9fb8fcdec), bundled for Ghostex with React, ReactDOM and modern-screenshot (MIT).
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
"use strict";var __ghostexAgentation=(()=>{var F5=Object.create;var ba=Object.defineProperty;var O5=Object.getOwnPropertyDescriptor;var B5=Object.getOwnPropertyNames;var z5=Object.getPrototypeOf,H5=Object.prototype.hasOwnProperty;var Oo=(e,t)=>()=>(t||e((t={exports:{}}).exports,t),t.exports),W5=(e,t)=>{for(var n in t)ba(e,n,{get:t[n],enumerable:!0})},ef=(e,t,n,o)=>{if(t&&typeof t=="object"||typeof t=="function")for(let r of B5(t))!H5.call(e,r)&&r!==n&&ba(e,r,{get:()=>t[r],enumerable:!(o=O5(t,r))||o.enumerable});return e};var ae=(e,t,n)=>(n=e!=null?F5(z5(e)):{},ef(t||!e||!e.__esModule?ba(n,"default",{value:e,enumerable:!0}):n,e)),U5=e=>ef(ba({},"__esModule",{value:!0}),e);var mf=Oo(Ue=>{"use strict";var $s=Symbol.for("react.element"),j5=Symbol.for("react.portal"),Y5=Symbol.for("react.fragment"),V5=Symbol.for("react.strict_mode"),X5=Symbol.for("react.profiler"),Q5=Symbol.for("react.provider"),G5=Symbol.for("react.context"),q5=Symbol.for("react.forward_ref"),K5=Symbol.for("react.suspense"),J5=Symbol.for("react.memo"),Z5=Symbol.for("react.lazy"),tf=Symbol.iterator;function e2(e){return e===null||typeof e!="object"?null:(e=tf&&e[tf]||e["@@iterator"],typeof e=="function"?e:null)}var rf={isMounted:function(){return!1},enqueueForceUpdate:function(){},enqueueReplaceState:function(){},enqueueSetState:function(){}},sf=Object.assign,lf={};function ki(e,t,n){this.props=e,this.context=t,this.refs=lf,this.updater=n||rf}ki.prototype.isReactComponent={};ki.prototype.setState=function(e,t){if(typeof e!="object"&&typeof e!="function"&&e!=null)throw Error("setState(...): takes an object of state variables to update or a function which returns an object of state variables.");this.updater.enqueueSetState(this,e,t,"setState")};ki.prototype.forceUpdate=function(e){this.updater.enqueueForceUpdate(this,e,"forceUpdate")};function af(){}af.prototype=ki.prototype;function Ju(e,t,n){this.props=e,this.context=t,this.refs=lf,this.updater=n||rf}var Zu=Ju.prototype=new af;Zu.constructor=Ju;sf(Zu,ki.prototype);Zu.isPureReactComponent=!0;var nf=Array.isArray,cf=Object.prototype.hasOwnProperty,ed={current:null},uf={key:!0,ref:!0,__self:!0,__source:!0};function df(e,t,n){var o,r={},i=null,s=null;if(t!=null)for(o in t.ref!==void 0&&(s=t.ref),t.key!==void 0&&(i=""+t.key),t)cf.call(t,o)&&!uf.hasOwnProperty(o)&&(r[o]=t[o]);var l=arguments.length-2;if(l===1)r.children=n;else if(1<l){for(var a=Array(l),c=0;c<l;c++)a[c]=arguments[c+2];r.children=a}if(e&&e.defaultProps)for(o in l=e.defaultProps,l)r[o]===void 0&&(r[o]=l[o]);return{$$typeof:$s,type:e,key:i,ref:s,props:r,_owner:ed.current}}function t2(e,t){return{$$typeof:$s,type:e.type,key:t,ref:e.ref,props:e.props,_owner:e._owner}}function td(e){return typeof e=="object"&&e!==null&&e.$$typeof===$s}function n2(e){var t={"=":"=0",":":"=2"};return"$"+e.replace(/[=:]/g,function(n){return t[n]})}var of=/\/+/g;function Ku(e,t){return typeof e=="object"&&e!==null&&e.key!=null?n2(""+e.key):t.toString(36)}function Sa(e,t,n,o,r){var i=typeof e;(i==="undefined"||i==="boolean")&&(e=null);var s=!1;if(e===null)s=!0;else switch(i){case"string":case"number":s=!0;break;case"object":switch(e.$$typeof){case $s:case j5:s=!0}}if(s)return s=e,r=r(s),e=o===""?"."+Ku(s,0):o,nf(r)?(n="",e!=null&&(n=e.replace(of,"$&/")+"/"),Sa(r,t,n,"",function(c){return c})):r!=null&&(td(r)&&(r=t2(r,n+(!r.key||s&&s.key===r.key?"":(""+r.key).replace(of,"$&/")+"/")+e)),t.push(r)),1;if(s=0,o=o===""?".":o+":",nf(e))for(var l=0;l<e.length;l++){i=e[l];var a=o+Ku(i,l);s+=Sa(i,t,n,a,r)}else if(a=e2(e),typeof a=="function")for(e=a.call(e),l=0;!(i=e.next()).done;)i=i.value,a=o+Ku(i,l++),s+=Sa(i,t,n,a,r);else if(i==="object")throw t=String(e),Error("Objects are not valid as a React child (found: "+(t==="[object Object]"?"object with keys {"+Object.keys(e).join(", ")+"}":t)+"). If you meant to render a collection of children, use an array instead.");return s}function ka(e,t,n){if(e==null)return e;var o=[],r=0;return Sa(e,o,"","",function(i){return t.call(n,i,r++)}),o}function o2(e){if(e._status===-1){var t=e._result;t=t(),t.then(function(n){(e._status===0||e._status===-1)&&(e._status=1,e._result=n)},function(n){(e._status===0||e._status===-1)&&(e._status=2,e._result=n)}),e._status===-1&&(e._status=0,e._result=t)}if(e._status===1)return e._result.default;throw e._result}var hn={current:null},Ca={transition:null},r2={ReactCurrentDispatcher:hn,ReactCurrentBatchConfig:Ca,ReactCurrentOwner:ed};function _f(){throw Error("act(...) is not supported in production builds of React.")}Ue.Children={map:ka,forEach:function(e,t,n){ka(e,function(){t.apply(this,arguments)},n)},count:function(e){var t=0;return ka(e,function(){t++}),t},toArray:function(e){return ka(e,function(t){return t})||[]},only:function(e){if(!td(e))throw Error("React.Children.only expected to receive a single React element child.");return e}};Ue.Component=ki;Ue.Fragment=Y5;Ue.Profiler=X5;Ue.PureComponent=Ju;Ue.StrictMode=V5;Ue.Suspense=K5;Ue.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED=r2;Ue.act=_f;Ue.cloneElement=function(e,t,n){if(e==null)throw Error("React.cloneElement(...): The argument must be a React element, but you passed "+e+".");var o=sf({},e.props),r=e.key,i=e.ref,s=e._owner;if(t!=null){if(t.ref!==void 0&&(i=t.ref,s=ed.current),t.key!==void 0&&(r=""+t.key),e.type&&e.type.defaultProps)var l=e.type.defaultProps;for(a in t)cf.call(t,a)&&!uf.hasOwnProperty(a)&&(o[a]=t[a]===void 0&&l!==void 0?l[a]:t[a])}var a=arguments.length-2;if(a===1)o.children=n;else if(1<a){l=Array(a);for(var c=0;c<a;c++)l[c]=arguments[c+2];o.children=l}return{$$typeof:$s,type:e.type,key:r,ref:i,props:o,_owner:s}};Ue.createContext=function(e){return e={$$typeof:G5,_currentValue:e,_currentValue2:e,_threadCount:0,Provider:null,Consumer:null,_defaultValue:null,_globalName:null},e.Provider={$$typeof:Q5,_context:e},e.Consumer=e};Ue.createElement=df;Ue.createFactory=function(e){var t=df.bind(null,e);return t.type=e,t};Ue.createRef=function(){return{current:null}};Ue.forwardRef=function(e){return{$$typeof:q5,render:e}};Ue.isValidElement=td;Ue.lazy=function(e){return{$$typeof:Z5,_payload:{_status:-1,_result:e},_init:o2}};Ue.memo=function(e,t){return{$$typeof:J5,type:e,compare:t===void 0?null:t}};Ue.startTransition=function(e){var t=Ca.transition;Ca.transition={};try{e()}finally{Ca.transition=t}};Ue.unstable_act=_f;Ue.useCallback=function(e,t){return hn.current.useCallback(e,t)};Ue.useContext=function(e){return hn.current.useContext(e)};Ue.useDebugValue=function(){};Ue.useDeferredValue=function(e){return hn.current.useDeferredValue(e)};Ue.useEffect=function(e,t){return hn.current.useEffect(e,t)};Ue.useId=function(){return hn.current.useId()};Ue.useImperativeHandle=function(e,t,n){return hn.current.useImperativeHandle(e,t,n)};Ue.useInsertionEffect=function(e,t){return hn.current.useInsertionEffect(e,t)};Ue.useLayoutEffect=function(e,t){return hn.current.useLayoutEffect(e,t)};Ue.useMemo=function(e,t){return hn.current.useMemo(e,t)};Ue.useReducer=function(e,t,n){return hn.current.useReducer(e,t,n)};Ue.useRef=function(e){return hn.current.useRef(e)};Ue.useState=function(e){return hn.current.useState(e)};Ue.useSyncExternalStore=function(e,t,n){return hn.current.useSyncExternalStore(e,t,n)};Ue.useTransition=function(){return hn.current.useTransition()};Ue.version="18.3.1"});var bt=Oo((Kk,pf)=>{"use strict";pf.exports=mf()});var Sf=Oo(kt=>{"use strict";function id(e,t){var n=e.length;e.push(t);e:for(;0<n;){var o=n-1>>>1,r=e[o];if(0<Ea(r,t))e[o]=t,e[n]=r,n=o;else break e}}function ro(e){return e.length===0?null:e[0]}function Ta(e){if(e.length===0)return null;var t=e[0],n=e.pop();if(n!==t){e[0]=n;e:for(var o=0,r=e.length,i=r>>>1;o<i;){var s=2*(o+1)-1,l=e[s],a=s+1,c=e[a];if(0>Ea(l,n))a<r&&0>Ea(c,l)?(e[o]=c,e[a]=n,o=a):(e[o]=l,e[s]=n,o=s);else if(a<r&&0>Ea(c,n))e[o]=c,e[a]=n,o=a;else break e}}return t}function Ea(e,t){var n=e.sortIndex-t.sortIndex;return n!==0?n:e.id-t.id}typeof performance=="object"&&typeof performance.now=="function"?(ff=performance,kt.unstable_now=function(){return ff.now()}):(nd=Date,gf=nd.now(),kt.unstable_now=function(){return nd.now()-gf});var ff,nd,gf,bo=[],nr=[],i2=1,Yn=null,un=3,La=!1,jr=!1,Os=!1,xf=typeof setTimeout=="function"?setTimeout:null,vf=typeof clearTimeout=="function"?clearTimeout:null,hf=typeof setImmediate<"u"?setImmediate:null;typeof navigator<"u"&&navigator.scheduling!==void 0&&navigator.scheduling.isInputPending!==void 0&&navigator.scheduling.isInputPending.bind(navigator.scheduling);function sd(e){for(var t=ro(nr);t!==null;){if(t.callback===null)Ta(nr);else if(t.startTime<=e)Ta(nr),t.sortIndex=t.expirationTime,id(bo,t);else break;t=ro(nr)}}function ld(e){if(Os=!1,sd(e),!jr)if(ro(bo)!==null)jr=!0,cd(ad);else{var t=ro(nr);t!==null&&ud(ld,t.startTime-e)}}function ad(e,t){jr=!1,Os&&(Os=!1,vf(Bs),Bs=-1),La=!0;var n=un;try{for(sd(t),Yn=ro(bo);Yn!==null&&(!(Yn.expirationTime>t)||e&&!kf());){var o=Yn.callback;if(typeof o=="function"){Yn.callback=null,un=Yn.priorityLevel;var r=o(Yn.expirationTime<=t);t=kt.unstable_now(),typeof r=="function"?Yn.callback=r:Yn===ro(bo)&&Ta(bo),sd(t)}else Ta(bo);Yn=ro(bo)}if(Yn!==null)var i=!0;else{var s=ro(nr);s!==null&&ud(ld,s.startTime-t),i=!1}return i}finally{Yn=null,un=n,La=!1}}var Ra=!1,Ma=null,Bs=-1,wf=5,bf=-1;function kf(){return!(kt.unstable_now()-bf<wf)}function od(){if(Ma!==null){var e=kt.unstable_now();bf=e;var t=!0;try{t=Ma(!0,e)}finally{t?Fs():(Ra=!1,Ma=null)}}else Ra=!1}var Fs;typeof hf=="function"?Fs=function(){hf(od)}:typeof MessageChannel<"u"?(rd=new MessageChannel,yf=rd.port2,rd.port1.onmessage=od,Fs=function(){yf.postMessage(null)}):Fs=function(){xf(od,0)};var rd,yf;function cd(e){Ma=e,Ra||(Ra=!0,Fs())}function ud(e,t){Bs=xf(function(){e(kt.unstable_now())},t)}kt.unstable_IdlePriority=5;kt.unstable_ImmediatePriority=1;kt.unstable_LowPriority=4;kt.unstable_NormalPriority=3;kt.unstable_Profiling=null;kt.unstable_UserBlockingPriority=2;kt.unstable_cancelCallback=function(e){e.callback=null};kt.unstable_continueExecution=function(){jr||La||(jr=!0,cd(ad))};kt.unstable_forceFrameRate=function(e){0>e||125<e?console.error("forceFrameRate takes a positive int between 0 and 125, forcing frame rates higher than 125 fps is not supported"):wf=0<e?Math.floor(1e3/e):5};kt.unstable_getCurrentPriorityLevel=function(){return un};kt.unstable_getFirstCallbackNode=function(){return ro(bo)};kt.unstable_next=function(e){switch(un){case 1:case 2:case 3:var t=3;break;default:t=un}var n=un;un=t;try{return e()}finally{un=n}};kt.unstable_pauseExecution=function(){};kt.unstable_requestPaint=function(){};kt.unstable_runWithPriority=function(e,t){switch(e){case 1:case 2:case 3:case 4:case 5:break;default:e=3}var n=un;un=e;try{return t()}finally{un=n}};kt.unstable_scheduleCallback=function(e,t,n){var o=kt.unstable_now();switch(typeof n=="object"&&n!==null?(n=n.delay,n=typeof n=="number"&&0<n?o+n:o):n=o,e){case 1:var r=-1;break;case 2:r=250;break;case 5:r=1073741823;break;case 4:r=1e4;break;default:r=5e3}return r=n+r,e={id:i2++,callback:t,priorityLevel:e,startTime:n,expirationTime:r,sortIndex:-1},n>o?(e.sortIndex=n,id(nr,e),ro(bo)===null&&e===ro(nr)&&(Os?(vf(Bs),Bs=-1):Os=!0,ud(ld,n-o))):(e.sortIndex=r,id(bo,e),jr||La||(jr=!0,cd(ad))),e};kt.unstable_shouldYield=kf;kt.unstable_wrapCallback=function(e){var t=un;return function(){var n=un;un=t;try{return e.apply(this,arguments)}finally{un=n}}}});var Ef=Oo((Zk,Cf)=>{"use strict";Cf.exports=Sf()});var R0=Oo(zn=>{"use strict";var s2=bt(),On=Ef();function Q(e){for(var t="https://reactjs.org/docs/error-decoder.html?invariant="+e,n=1;n<arguments.length;n++)t+="&args[]="+encodeURIComponent(arguments[n]);return"Minified React error #"+e+"; visit "+t+" for the full message or use the non-minified dev environment for full errors and additional helpful warnings."}var Ig=new Set,ll={};function oi(e,t){ji(e,t),ji(e+"Capture",t)}function ji(e,t){for(ll[e]=t,e=0;e<t.length;e++)Ig.add(t[e])}var jo=!(typeof window>"u"||typeof window.document>"u"||typeof window.document.createElement>"u"),Id=Object.prototype.hasOwnProperty,l2=/^[:A-Z_a-z\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u02FF\u0370-\u037D\u037F-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD][:A-Z_a-z\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u02FF\u0370-\u037D\u037F-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD\-.0-9\u00B7\u0300-\u036F\u203F-\u2040]*$/,Mf={},Tf={};function a2(e){return Id.call(Tf,e)?!0:Id.call(Mf,e)?!1:l2.test(e)?Tf[e]=!0:(Mf[e]=!0,!1)}function c2(e,t,n,o){if(n!==null&&n.type===0)return!1;switch(typeof t){case"function":case"symbol":return!0;case"boolean":return o?!1:n!==null?!n.acceptsBooleans:(e=e.toLowerCase().slice(0,5),e!=="data-"&&e!=="aria-");default:return!1}}function u2(e,t,n,o){if(t===null||typeof t>"u"||c2(e,t,n,o))return!0;if(o)return!1;if(n!==null)switch(n.type){case 3:return!t;case 4:return t===!1;case 5:return isNaN(t);case 6:return isNaN(t)||1>t}return!1}function vn(e,t,n,o,r,i,s){this.acceptsBooleans=t===2||t===3||t===4,this.attributeName=o,this.attributeNamespace=r,this.mustUseProperty=n,this.propertyName=e,this.type=t,this.sanitizeURL=i,this.removeEmptyString=s}var rn={};"children dangerouslySetInnerHTML defaultValue defaultChecked innerHTML suppressContentEditableWarning suppressHydrationWarning style".split(" ").forEach(function(e){rn[e]=new vn(e,0,!1,e,null,!1,!1)});[["acceptCharset","accept-charset"],["className","class"],["htmlFor","for"],["httpEquiv","http-equiv"]].forEach(function(e){var t=e[0];rn[t]=new vn(t,1,!1,e[1],null,!1,!1)});["contentEditable","draggable","spellCheck","value"].forEach(function(e){rn[e]=new vn(e,2,!1,e.toLowerCase(),null,!1,!1)});["autoReverse","externalResourcesRequired","focusable","preserveAlpha"].forEach(function(e){rn[e]=new vn(e,2,!1,e,null,!1,!1)});"allowFullScreen async autoFocus autoPlay controls default defer disabled disablePictureInPicture disableRemotePlayback formNoValidate hidden loop noModule noValidate open playsInline readOnly required reversed scoped seamless itemScope".split(" ").forEach(function(e){rn[e]=new vn(e,3,!1,e.toLowerCase(),null,!1,!1)});["checked","multiple","muted","selected"].forEach(function(e){rn[e]=new vn(e,3,!0,e,null,!1,!1)});["capture","download"].forEach(function(e){rn[e]=new vn(e,4,!1,e,null,!1,!1)});["cols","rows","size","span"].forEach(function(e){rn[e]=new vn(e,6,!1,e,null,!1,!1)});["rowSpan","start"].forEach(function(e){rn[e]=new vn(e,5,!1,e.toLowerCase(),null,!1,!1)});var C_=/[\-:]([a-z])/g;function E_(e){return e[1].toUpperCase()}"accent-height alignment-baseline arabic-form baseline-shift cap-height clip-path clip-rule color-interpolation color-interpolation-filters color-profile color-rendering dominant-baseline enable-background fill-opacity fill-rule flood-color flood-opacity font-family font-size font-size-adjust font-stretch font-style font-variant font-weight glyph-name glyph-orientation-horizontal glyph-orientation-vertical horiz-adv-x horiz-origin-x image-rendering letter-spacing lighting-color marker-end marker-mid marker-start overline-position overline-thickness paint-order panose-1 pointer-events rendering-intent shape-rendering stop-color stop-opacity strikethrough-position strikethrough-thickness stroke-dasharray stroke-dashoffset stroke-linecap stroke-linejoin stroke-miterlimit stroke-opacity stroke-width text-anchor text-decoration text-rendering underline-position underline-thickness unicode-bidi unicode-range units-per-em v-alphabetic v-hanging v-ideographic v-mathematical vector-effect vert-adv-y vert-origin-x vert-origin-y word-spacing writing-mode xmlns:xlink x-height".split(" ").forEach(function(e){var t=e.replace(C_,E_);rn[t]=new vn(t,1,!1,e,null,!1,!1)});"xlink:actuate xlink:arcrole xlink:role xlink:show xlink:title xlink:type".split(" ").forEach(function(e){var t=e.replace(C_,E_);rn[t]=new vn(t,1,!1,e,"http://www.w3.org/1999/xlink",!1,!1)});["xml:base","xml:lang","xml:space"].forEach(function(e){var t=e.replace(C_,E_);rn[t]=new vn(t,1,!1,e,"http://www.w3.org/XML/1998/namespace",!1,!1)});["tabIndex","crossOrigin"].forEach(function(e){rn[e]=new vn(e,1,!1,e.toLowerCase(),null,!1,!1)});rn.xlinkHref=new vn("xlinkHref",1,!1,"xlink:href","http://www.w3.org/1999/xlink",!0,!1);["src","href","action","formAction"].forEach(function(e){rn[e]=new vn(e,1,!1,e.toLowerCase(),null,!0,!0)});function M_(e,t,n,o){var r=rn.hasOwnProperty(t)?rn[t]:null;(r!==null?r.type!==0:o||!(2<t.length)||t[0]!=="o"&&t[0]!=="O"||t[1]!=="n"&&t[1]!=="N")&&(u2(t,n,r,o)&&(n=null),o||r===null?a2(t)&&(n===null?e.removeAttribute(t):e.setAttribute(t,""+n)):r.mustUseProperty?e[r.propertyName]=n===null?r.type===3?!1:"":n:(t=r.attributeName,o=r.attributeNamespace,n===null?e.removeAttribute(t):(r=r.type,n=r===3||r===4&&n===!0?"":""+n,o?e.setAttributeNS(o,t,n):e.setAttribute(t,n))))}var Qo=s2.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED,Pa=Symbol.for("react.element"),Ei=Symbol.for("react.portal"),Mi=Symbol.for("react.fragment"),T_=Symbol.for("react.strict_mode"),Dd=Symbol.for("react.profiler"),Dg=Symbol.for("react.provider"),Ag=Symbol.for("react.context"),L_=Symbol.for("react.forward_ref"),Ad=Symbol.for("react.suspense"),$d=Symbol.for("react.suspense_list"),R_=Symbol.for("react.memo"),rr=Symbol.for("react.lazy"),$g=Symbol.for("react.offscreen"),Lf=Symbol.iterator;function zs(e){return e===null||typeof e!="object"?null:(e=Lf&&e[Lf]||e["@@iterator"],typeof e=="function"?e:null)}var It=Object.assign,dd;function Qs(e){if(dd===void 0)try{throw Error()}catch(n){var t=n.stack.trim().match(/\n( *(at )?)/);dd=t&&t[1]||""}return`
`+dd+e}var _d=!1;function md(e,t){if(!e||_d)return"";_d=!0;var n=Error.prepareStackTrace;Error.prepareStackTrace=void 0;try{if(t)if(t=function(){throw Error()},Object.defineProperty(t.prototype,"props",{set:function(){throw Error()}}),typeof Reflect=="object"&&Reflect.construct){try{Reflect.construct(t,[])}catch(c){var o=c}Reflect.construct(e,[],t)}else{try{t.call()}catch(c){o=c}e.call(t.prototype)}else{try{throw Error()}catch(c){o=c}e()}}catch(c){if(c&&o&&typeof c.stack=="string"){for(var r=c.stack.split(`
`),i=o.stack.split(`
`),s=r.length-1,l=i.length-1;1<=s&&0<=l&&r[s]!==i[l];)l--;for(;1<=s&&0<=l;s--,l--)if(r[s]!==i[l]){if(s!==1||l!==1)do if(s--,l--,0>l||r[s]!==i[l]){var a=`
`+r[s].replace(" at new "," at ");return e.displayName&&a.includes("<anonymous>")&&(a=a.replace("<anonymous>",e.displayName)),a}while(1<=s&&0<=l);break}}}finally{_d=!1,Error.prepareStackTrace=n}return(e=e?e.displayName||e.name:"")?Qs(e):""}function d2(e){switch(e.tag){case 5:return Qs(e.type);case 16:return Qs("Lazy");case 13:return Qs("Suspense");case 19:return Qs("SuspenseList");case 0:case 2:case 15:return e=md(e.type,!1),e;case 11:return e=md(e.type.render,!1),e;case 1:return e=md(e.type,!0),e;default:return""}}function Fd(e){if(e==null)return null;if(typeof e=="function")return e.displayName||e.name||null;if(typeof e=="string")return e;switch(e){case Mi:return"Fragment";case Ei:return"Portal";case Dd:return"Profiler";case T_:return"StrictMode";case Ad:return"Suspense";case $d:return"SuspenseList"}if(typeof e=="object")switch(e.$$typeof){case Ag:return(e.displayName||"Context")+".Consumer";case Dg:return(e._context.displayName||"Context")+".Provider";case L_:var t=e.render;return e=e.displayName,e||(e=t.displayName||t.name||"",e=e!==""?"ForwardRef("+e+")":"ForwardRef"),e;case R_:return t=e.displayName||null,t!==null?t:Fd(e.type)||"Memo";case rr:t=e._payload,e=e._init;try{return Fd(e(t))}catch{}}return null}function _2(e){var t=e.type;switch(e.tag){case 24:return"Cache";case 9:return(t.displayName||"Context")+".Consumer";case 10:return(t._context.displayName||"Context")+".Provider";case 18:return"DehydratedFragment";case 11:return e=t.render,e=e.displayName||e.name||"",t.displayName||(e!==""?"ForwardRef("+e+")":"ForwardRef");case 7:return"Fragment";case 5:return t;case 4:return"Portal";case 3:return"Root";case 6:return"Text";case 16:return Fd(t);case 8:return t===T_?"StrictMode":"Mode";case 22:return"Offscreen";case 12:return"Profiler";case 21:return"Scope";case 13:return"Suspense";case 19:return"SuspenseList";case 25:return"TracingMarker";case 1:case 0:case 17:case 2:case 14:case 15:if(typeof t=="function")return t.displayName||t.name||null;if(typeof t=="string")return t}return null}function yr(e){switch(typeof e){case"boolean":case"number":case"string":case"undefined":return e;case"object":return e;default:return""}}function Fg(e){var t=e.type;return(e=e.nodeName)&&e.toLowerCase()==="input"&&(t==="checkbox"||t==="radio")}function m2(e){var t=Fg(e)?"checked":"value",n=Object.getOwnPropertyDescriptor(e.constructor.prototype,t),o=""+e[t];if(!e.hasOwnProperty(t)&&typeof n<"u"&&typeof n.get=="function"&&typeof n.set=="function"){var r=n.get,i=n.set;return Object.defineProperty(e,t,{configurable:!0,get:function(){return r.call(this)},set:function(s){o=""+s,i.call(this,s)}}),Object.defineProperty(e,t,{enumerable:n.enumerable}),{getValue:function(){return o},setValue:function(s){o=""+s},stopTracking:function(){e._valueTracker=null,delete e[t]}}}}function Na(e){e._valueTracker||(e._valueTracker=m2(e))}function Og(e){if(!e)return!1;var t=e._valueTracker;if(!t)return!0;var n=t.getValue(),o="";return e&&(o=Fg(e)?e.checked?"true":"false":e.value),e=o,e!==n?(t.setValue(e),!0):!1}function sc(e){if(e=e||(typeof document<"u"?document:void 0),typeof e>"u")return null;try{return e.activeElement||e.body}catch{return e.body}}function Od(e,t){var n=t.checked;return It({},t,{defaultChecked:void 0,defaultValue:void 0,value:void 0,checked:n??e._wrapperState.initialChecked})}function Rf(e,t){var n=t.defaultValue==null?"":t.defaultValue,o=t.checked!=null?t.checked:t.defaultChecked;n=yr(t.value!=null?t.value:n),e._wrapperState={initialChecked:o,initialValue:n,controlled:t.type==="checkbox"||t.type==="radio"?t.checked!=null:t.value!=null}}function Bg(e,t){t=t.checked,t!=null&&M_(e,"checked",t,!1)}function Bd(e,t){Bg(e,t);var n=yr(t.value),o=t.type;if(n!=null)o==="number"?(n===0&&e.value===""||e.value!=n)&&(e.value=""+n):e.value!==""+n&&(e.value=""+n);else if(o==="submit"||o==="reset"){e.removeAttribute("value");return}t.hasOwnProperty("value")?zd(e,t.type,n):t.hasOwnProperty("defaultValue")&&zd(e,t.type,yr(t.defaultValue)),t.checked==null&&t.defaultChecked!=null&&(e.defaultChecked=!!t.defaultChecked)}function Pf(e,t,n){if(t.hasOwnProperty("value")||t.hasOwnProperty("defaultValue")){var o=t.type;if(!(o!=="submit"&&o!=="reset"||t.value!==void 0&&t.value!==null))return;t=""+e._wrapperState.initialValue,n||t===e.value||(e.value=t),e.defaultValue=t}n=e.name,n!==""&&(e.name=""),e.defaultChecked=!!e._wrapperState.initialChecked,n!==""&&(e.name=n)}function zd(e,t,n){(t!=="number"||sc(e.ownerDocument)!==e)&&(n==null?e.defaultValue=""+e._wrapperState.initialValue:e.defaultValue!==""+n&&(e.defaultValue=""+n))}var Gs=Array.isArray;function Oi(e,t,n,o){if(e=e.options,t){t={};for(var r=0;r<n.length;r++)t["$"+n[r]]=!0;for(n=0;n<e.length;n++)r=t.hasOwnProperty("$"+e[n].value),e[n].selected!==r&&(e[n].selected=r),r&&o&&(e[n].defaultSelected=!0)}else{for(n=""+yr(n),t=null,r=0;r<e.length;r++){if(e[r].value===n){e[r].selected=!0,o&&(e[r].defaultSelected=!0);return}t!==null||e[r].disabled||(t=e[r])}t!==null&&(t.selected=!0)}}function Hd(e,t){if(t.dangerouslySetInnerHTML!=null)throw Error(Q(91));return It({},t,{value:void 0,defaultValue:void 0,children:""+e._wrapperState.initialValue})}function Nf(e,t){var n=t.value;if(n==null){if(n=t.children,t=t.defaultValue,n!=null){if(t!=null)throw Error(Q(92));if(Gs(n)){if(1<n.length)throw Error(Q(93));n=n[0]}t=n}t==null&&(t=""),n=t}e._wrapperState={initialValue:yr(n)}}function zg(e,t){var n=yr(t.value),o=yr(t.defaultValue);n!=null&&(n=""+n,n!==e.value&&(e.value=n),t.defaultValue==null&&e.defaultValue!==n&&(e.defaultValue=n)),o!=null&&(e.defaultValue=""+o)}function If(e){var t=e.textContent;t===e._wrapperState.initialValue&&t!==""&&t!==null&&(e.value=t)}function Hg(e){switch(e){case"svg":return"http://www.w3.org/2000/svg";case"math":return"http://www.w3.org/1998/Math/MathML";default:return"http://www.w3.org/1999/xhtml"}}function Wd(e,t){return e==null||e==="http://www.w3.org/1999/xhtml"?Hg(t):e==="http://www.w3.org/2000/svg"&&t==="foreignObject"?"http://www.w3.org/1999/xhtml":e}var Ia,Wg=(function(e){return typeof MSApp<"u"&&MSApp.execUnsafeLocalFunction?function(t,n,o,r){MSApp.execUnsafeLocalFunction(function(){return e(t,n,o,r)})}:e})(function(e,t){if(e.namespaceURI!=="http://www.w3.org/2000/svg"||"innerHTML"in e)e.innerHTML=t;else{for(Ia=Ia||document.createElement("div"),Ia.innerHTML="<svg>"+t.valueOf().toString()+"</svg>",t=Ia.firstChild;e.firstChild;)e.removeChild(e.firstChild);for(;t.firstChild;)e.appendChild(t.firstChild)}});function al(e,t){if(t){var n=e.firstChild;if(n&&n===e.lastChild&&n.nodeType===3){n.nodeValue=t;return}}e.textContent=t}var Js={animationIterationCount:!0,aspectRatio:!0,borderImageOutset:!0,borderImageSlice:!0,borderImageWidth:!0,boxFlex:!0,boxFlexGroup:!0,boxOrdinalGroup:!0,columnCount:!0,columns:!0,flex:!0,flexGrow:!0,flexPositive:!0,flexShrink:!0,flexNegative:!0,flexOrder:!0,gridArea:!0,gridRow:!0,gridRowEnd:!0,gridRowSpan:!0,gridRowStart:!0,gridColumn:!0,gridColumnEnd:!0,gridColumnSpan:!0,gridColumnStart:!0,fontWeight:!0,lineClamp:!0,lineHeight:!0,opacity:!0,order:!0,orphans:!0,tabSize:!0,widows:!0,zIndex:!0,zoom:!0,fillOpacity:!0,floodOpacity:!0,stopOpacity:!0,strokeDasharray:!0,strokeDashoffset:!0,strokeMiterlimit:!0,strokeOpacity:!0,strokeWidth:!0},p2=["Webkit","ms","Moz","O"];Object.keys(Js).forEach(function(e){p2.forEach(function(t){t=t+e.charAt(0).toUpperCase()+e.substring(1),Js[t]=Js[e]})});function Ug(e,t,n){return t==null||typeof t=="boolean"||t===""?"":n||typeof t!="number"||t===0||Js.hasOwnProperty(e)&&Js[e]?(""+t).trim():t+"px"}function jg(e,t){e=e.style;for(var n in t)if(t.hasOwnProperty(n)){var o=n.indexOf("--")===0,r=Ug(n,t[n],o);n==="float"&&(n="cssFloat"),o?e.setProperty(n,r):e[n]=r}}var f2=It({menuitem:!0},{area:!0,base:!0,br:!0,col:!0,embed:!0,hr:!0,img:!0,input:!0,keygen:!0,link:!0,meta:!0,param:!0,source:!0,track:!0,wbr:!0});function Ud(e,t){if(t){if(f2[e]&&(t.children!=null||t.dangerouslySetInnerHTML!=null))throw Error(Q(137,e));if(t.dangerouslySetInnerHTML!=null){if(t.children!=null)throw Error(Q(60));if(typeof t.dangerouslySetInnerHTML!="object"||!("__html"in t.dangerouslySetInnerHTML))throw Error(Q(61))}if(t.style!=null&&typeof t.style!="object")throw Error(Q(62))}}function jd(e,t){if(e.indexOf("-")===-1)return typeof t.is=="string";switch(e){case"annotation-xml":case"color-profile":case"font-face":case"font-face-src":case"font-face-uri":case"font-face-format":case"font-face-name":case"missing-glyph":return!1;default:return!0}}var Yd=null;function P_(e){return e=e.target||e.srcElement||window,e.correspondingUseElement&&(e=e.correspondingUseElement),e.nodeType===3?e.parentNode:e}var Vd=null,Bi=null,zi=null;function Df(e){if(e=El(e)){if(typeof Vd!="function")throw Error(Q(280));var t=e.stateNode;t&&(t=Dc(t),Vd(e.stateNode,e.type,t))}}function Yg(e){Bi?zi?zi.push(e):zi=[e]:Bi=e}function Vg(){if(Bi){var e=Bi,t=zi;if(zi=Bi=null,Df(e),t)for(e=0;e<t.length;e++)Df(t[e])}}function Xg(e,t){return e(t)}function Qg(){}var pd=!1;function Gg(e,t,n){if(pd)return e(t,n);pd=!0;try{return Xg(e,t,n)}finally{pd=!1,(Bi!==null||zi!==null)&&(Qg(),Vg())}}function cl(e,t){var n=e.stateNode;if(n===null)return null;var o=Dc(n);if(o===null)return null;n=o[t];e:switch(t){case"onClick":case"onClickCapture":case"onDoubleClick":case"onDoubleClickCapture":case"onMouseDown":case"onMouseDownCapture":case"onMouseMove":case"onMouseMoveCapture":case"onMouseUp":case"onMouseUpCapture":case"onMouseEnter":(o=!o.disabled)||(e=e.type,o=!(e==="button"||e==="input"||e==="select"||e==="textarea")),e=!o;break e;default:e=!1}if(e)return null;if(n&&typeof n!="function")throw Error(Q(231,t,typeof n));return n}var Xd=!1;if(jo)try{Si={},Object.defineProperty(Si,"passive",{get:function(){Xd=!0}}),window.addEventListener("test",Si,Si),window.removeEventListener("test",Si,Si)}catch{Xd=!1}var Si;function g2(e,t,n,o,r,i,s,l,a){var c=Array.prototype.slice.call(arguments,3);try{t.apply(n,c)}catch(d){this.onError(d)}}var Zs=!1,lc=null,ac=!1,Qd=null,h2={onError:function(e){Zs=!0,lc=e}};function y2(e,t,n,o,r,i,s,l,a){Zs=!1,lc=null,g2.apply(h2,arguments)}function x2(e,t,n,o,r,i,s,l,a){if(y2.apply(this,arguments),Zs){if(Zs){var c=lc;Zs=!1,lc=null}else throw Error(Q(198));ac||(ac=!0,Qd=c)}}function ri(e){var t=e,n=e;if(e.alternate)for(;t.return;)t=t.return;else{e=t;do t=e,(t.flags&4098)!==0&&(n=t.return),e=t.return;while(e)}return t.tag===3?n:null}function qg(e){if(e.tag===13){var t=e.memoizedState;if(t===null&&(e=e.alternate,e!==null&&(t=e.memoizedState)),t!==null)return t.dehydrated}return null}function Af(e){if(ri(e)!==e)throw Error(Q(188))}function v2(e){var t=e.alternate;if(!t){if(t=ri(e),t===null)throw Error(Q(188));return t!==e?null:e}for(var n=e,o=t;;){var r=n.return;if(r===null)break;var i=r.alternate;if(i===null){if(o=r.return,o!==null){n=o;continue}break}if(r.child===i.child){for(i=r.child;i;){if(i===n)return Af(r),e;if(i===o)return Af(r),t;i=i.sibling}throw Error(Q(188))}if(n.return!==o.return)n=r,o=i;else{for(var s=!1,l=r.child;l;){if(l===n){s=!0,n=r,o=i;break}if(l===o){s=!0,o=r,n=i;break}l=l.sibling}if(!s){for(l=i.child;l;){if(l===n){s=!0,n=i,o=r;break}if(l===o){s=!0,o=i,n=r;break}l=l.sibling}if(!s)throw Error(Q(189))}}if(n.alternate!==o)throw Error(Q(190))}if(n.tag!==3)throw Error(Q(188));return n.stateNode.current===n?e:t}function Kg(e){return e=v2(e),e!==null?Jg(e):null}function Jg(e){if(e.tag===5||e.tag===6)return e;for(e=e.child;e!==null;){var t=Jg(e);if(t!==null)return t;e=e.sibling}return null}var Zg=On.unstable_scheduleCallback,$f=On.unstable_cancelCallback,w2=On.unstable_shouldYield,b2=On.unstable_requestPaint,Ot=On.unstable_now,k2=On.unstable_getCurrentPriorityLevel,N_=On.unstable_ImmediatePriority,eh=On.unstable_UserBlockingPriority,cc=On.unstable_NormalPriority,S2=On.unstable_LowPriority,th=On.unstable_IdlePriority,Rc=null,Eo=null;function C2(e){if(Eo&&typeof Eo.onCommitFiberRoot=="function")try{Eo.onCommitFiberRoot(Rc,e,void 0,(e.current.flags&128)===128)}catch{}}var co=Math.clz32?Math.clz32:T2,E2=Math.log,M2=Math.LN2;function T2(e){return e>>>=0,e===0?32:31-(E2(e)/M2|0)|0}var Da=64,Aa=4194304;function qs(e){switch(e&-e){case 1:return 1;case 2:return 2;case 4:return 4;case 8:return 8;case 16:return 16;case 32:return 32;case 64:case 128:case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:case 262144:case 524288:case 1048576:case 2097152:return e&4194240;case 4194304:case 8388608:case 16777216:case 33554432:case 67108864:return e&130023424;case 134217728:return 134217728;case 268435456:return 268435456;case 536870912:return 536870912;case 1073741824:return 1073741824;default:return e}}function uc(e,t){var n=e.pendingLanes;if(n===0)return 0;var o=0,r=e.suspendedLanes,i=e.pingedLanes,s=n&268435455;if(s!==0){var l=s&~r;l!==0?o=qs(l):(i&=s,i!==0&&(o=qs(i)))}else s=n&~r,s!==0?o=qs(s):i!==0&&(o=qs(i));if(o===0)return 0;if(t!==0&&t!==o&&(t&r)===0&&(r=o&-o,i=t&-t,r>=i||r===16&&(i&4194240)!==0))return t;if((o&4)!==0&&(o|=n&16),t=e.entangledLanes,t!==0)for(e=e.entanglements,t&=o;0<t;)n=31-co(t),r=1<<n,o|=e[n],t&=~r;return o}function L2(e,t){switch(e){case 1:case 2:case 4:return t+250;case 8:case 16:case 32:case 64:case 128:case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:case 262144:case 524288:case 1048576:case 2097152:return t+5e3;case 4194304:case 8388608:case 16777216:case 33554432:case 67108864:return-1;case 134217728:case 268435456:case 536870912:case 1073741824:return-1;default:return-1}}function R2(e,t){for(var n=e.suspendedLanes,o=e.pingedLanes,r=e.expirationTimes,i=e.pendingLanes;0<i;){var s=31-co(i),l=1<<s,a=r[s];a===-1?((l&n)===0||(l&o)!==0)&&(r[s]=L2(l,t)):a<=t&&(e.expiredLanes|=l),i&=~l}}function Gd(e){return e=e.pendingLanes&-1073741825,e!==0?e:e&1073741824?1073741824:0}function nh(){var e=Da;return Da<<=1,(Da&4194240)===0&&(Da=64),e}function fd(e){for(var t=[],n=0;31>n;n++)t.push(e);return t}function Sl(e,t,n){e.pendingLanes|=t,t!==536870912&&(e.suspendedLanes=0,e.pingedLanes=0),e=e.eventTimes,t=31-co(t),e[t]=n}function P2(e,t){var n=e.pendingLanes&~t;e.pendingLanes=t,e.suspendedLanes=0,e.pingedLanes=0,e.expiredLanes&=t,e.mutableReadLanes&=t,e.entangledLanes&=t,t=e.entanglements;var o=e.eventTimes;for(e=e.expirationTimes;0<n;){var r=31-co(n),i=1<<r;t[r]=0,o[r]=-1,e[r]=-1,n&=~i}}function I_(e,t){var n=e.entangledLanes|=t;for(e=e.entanglements;n;){var o=31-co(n),r=1<<o;r&t|e[o]&t&&(e[o]|=t),n&=~r}}var _t=0;function oh(e){return e&=-e,1<e?4<e?(e&268435455)!==0?16:536870912:4:1}var rh,D_,ih,sh,lh,qd=!1,$a=[],ur=null,dr=null,_r=null,ul=new Map,dl=new Map,sr=[],N2="mousedown mouseup touchcancel touchend touchstart auxclick dblclick pointercancel pointerdown pointerup dragend dragstart drop compositionend compositionstart keydown keypress keyup input textInput copy cut paste click change contextmenu reset submit".split(" ");function Ff(e,t){switch(e){case"focusin":case"focusout":ur=null;break;case"dragenter":case"dragleave":dr=null;break;case"mouseover":case"mouseout":_r=null;break;case"pointerover":case"pointerout":ul.delete(t.pointerId);break;case"gotpointercapture":case"lostpointercapture":dl.delete(t.pointerId)}}function Hs(e,t,n,o,r,i){return e===null||e.nativeEvent!==i?(e={blockedOn:t,domEventName:n,eventSystemFlags:o,nativeEvent:i,targetContainers:[r]},t!==null&&(t=El(t),t!==null&&D_(t)),e):(e.eventSystemFlags|=o,t=e.targetContainers,r!==null&&t.indexOf(r)===-1&&t.push(r),e)}function I2(e,t,n,o,r){switch(t){case"focusin":return ur=Hs(ur,e,t,n,o,r),!0;case"dragenter":return dr=Hs(dr,e,t,n,o,r),!0;case"mouseover":return _r=Hs(_r,e,t,n,o,r),!0;case"pointerover":var i=r.pointerId;return ul.set(i,Hs(ul.get(i)||null,e,t,n,o,r)),!0;case"gotpointercapture":return i=r.pointerId,dl.set(i,Hs(dl.get(i)||null,e,t,n,o,r)),!0}return!1}function ah(e){var t=Xr(e.target);if(t!==null){var n=ri(t);if(n!==null){if(t=n.tag,t===13){if(t=qg(n),t!==null){e.blockedOn=t,lh(e.priority,function(){ih(n)});return}}else if(t===3&&n.stateNode.current.memoizedState.isDehydrated){e.blockedOn=n.tag===3?n.stateNode.containerInfo:null;return}}}e.blockedOn=null}function qa(e){if(e.blockedOn!==null)return!1;for(var t=e.targetContainers;0<t.length;){var n=Kd(e.domEventName,e.eventSystemFlags,t[0],e.nativeEvent);if(n===null){n=e.nativeEvent;var o=new n.constructor(n.type,n);Yd=o,n.target.dispatchEvent(o),Yd=null}else return t=El(n),t!==null&&D_(t),e.blockedOn=n,!1;t.shift()}return!0}function Of(e,t,n){qa(e)&&n.delete(t)}function D2(){qd=!1,ur!==null&&qa(ur)&&(ur=null),dr!==null&&qa(dr)&&(dr=null),_r!==null&&qa(_r)&&(_r=null),ul.forEach(Of),dl.forEach(Of)}function Ws(e,t){e.blockedOn===t&&(e.blockedOn=null,qd||(qd=!0,On.unstable_scheduleCallback(On.unstable_NormalPriority,D2)))}function _l(e){function t(r){return Ws(r,e)}if(0<$a.length){Ws($a[0],e);for(var n=1;n<$a.length;n++){var o=$a[n];o.blockedOn===e&&(o.blockedOn=null)}}for(ur!==null&&Ws(ur,e),dr!==null&&Ws(dr,e),_r!==null&&Ws(_r,e),ul.forEach(t),dl.forEach(t),n=0;n<sr.length;n++)o=sr[n],o.blockedOn===e&&(o.blockedOn=null);for(;0<sr.length&&(n=sr[0],n.blockedOn===null);)ah(n),n.blockedOn===null&&sr.shift()}var Hi=Qo.ReactCurrentBatchConfig,dc=!0;function A2(e,t,n,o){var r=_t,i=Hi.transition;Hi.transition=null;try{_t=1,A_(e,t,n,o)}finally{_t=r,Hi.transition=i}}function $2(e,t,n,o){var r=_t,i=Hi.transition;Hi.transition=null;try{_t=4,A_(e,t,n,o)}finally{_t=r,Hi.transition=i}}function A_(e,t,n,o){if(dc){var r=Kd(e,t,n,o);if(r===null)bd(e,t,o,_c,n),Ff(e,o);else if(I2(r,e,t,n,o))o.stopPropagation();else if(Ff(e,o),t&4&&-1<N2.indexOf(e)){for(;r!==null;){var i=El(r);if(i!==null&&rh(i),i=Kd(e,t,n,o),i===null&&bd(e,t,o,_c,n),i===r)break;r=i}r!==null&&o.stopPropagation()}else bd(e,t,o,null,n)}}var _c=null;function Kd(e,t,n,o){if(_c=null,e=P_(o),e=Xr(e),e!==null)if(t=ri(e),t===null)e=null;else if(n=t.tag,n===13){if(e=qg(t),e!==null)return e;e=null}else if(n===3){if(t.stateNode.current.memoizedState.isDehydrated)return t.tag===3?t.stateNode.containerInfo:null;e=null}else t!==e&&(e=null);return _c=e,null}function ch(e){switch(e){case"cancel":case"click":case"close":case"contextmenu":case"copy":case"cut":case"auxclick":case"dblclick":case"dragend":case"dragstart":case"drop":case"focusin":case"focusout":case"input":case"invalid":case"keydown":case"keypress":case"keyup":case"mousedown":case"mouseup":case"paste":case"pause":case"play":case"pointercancel":case"pointerdown":case"pointerup":case"ratechange":case"reset":case"resize":case"seeked":case"submit":case"touchcancel":case"touchend":case"touchstart":case"volumechange":case"change":case"selectionchange":case"textInput":case"compositionstart":case"compositionend":case"compositionupdate":case"beforeblur":case"afterblur":case"beforeinput":case"blur":case"fullscreenchange":case"focus":case"hashchange":case"popstate":case"select":case"selectstart":return 1;case"drag":case"dragenter":case"dragexit":case"dragleave":case"dragover":case"mousemove":case"mouseout":case"mouseover":case"pointermove":case"pointerout":case"pointerover":case"scroll":case"toggle":case"touchmove":case"wheel":case"mouseenter":case"mouseleave":case"pointerenter":case"pointerleave":return 4;case"message":switch(k2()){case N_:return 1;case eh:return 4;case cc:case S2:return 16;case th:return 536870912;default:return 16}default:return 16}}var ar=null,$_=null,Ka=null;function uh(){if(Ka)return Ka;var e,t=$_,n=t.length,o,r="value"in ar?ar.value:ar.textContent,i=r.length;for(e=0;e<n&&t[e]===r[e];e++);var s=n-e;for(o=1;o<=s&&t[n-o]===r[i-o];o++);return Ka=r.slice(e,1<o?1-o:void 0)}function Ja(e){var t=e.keyCode;return"charCode"in e?(e=e.charCode,e===0&&t===13&&(e=13)):e=t,e===10&&(e=13),32<=e||e===13?e:0}function Fa(){return!0}function Bf(){return!1}function Bn(e){function t(n,o,r,i,s){this._reactName=n,this._targetInst=r,this.type=o,this.nativeEvent=i,this.target=s,this.currentTarget=null;for(var l in e)e.hasOwnProperty(l)&&(n=e[l],this[l]=n?n(i):i[l]);return this.isDefaultPrevented=(i.defaultPrevented!=null?i.defaultPrevented:i.returnValue===!1)?Fa:Bf,this.isPropagationStopped=Bf,this}return It(t.prototype,{preventDefault:function(){this.defaultPrevented=!0;var n=this.nativeEvent;n&&(n.preventDefault?n.preventDefault():typeof n.returnValue!="unknown"&&(n.returnValue=!1),this.isDefaultPrevented=Fa)},stopPropagation:function(){var n=this.nativeEvent;n&&(n.stopPropagation?n.stopPropagation():typeof n.cancelBubble!="unknown"&&(n.cancelBubble=!0),this.isPropagationStopped=Fa)},persist:function(){},isPersistent:Fa}),t}var Ki={eventPhase:0,bubbles:0,cancelable:0,timeStamp:function(e){return e.timeStamp||Date.now()},defaultPrevented:0,isTrusted:0},F_=Bn(Ki),Cl=It({},Ki,{view:0,detail:0}),F2=Bn(Cl),gd,hd,Us,Pc=It({},Cl,{screenX:0,screenY:0,clientX:0,clientY:0,pageX:0,pageY:0,ctrlKey:0,shiftKey:0,altKey:0,metaKey:0,getModifierState:O_,button:0,buttons:0,relatedTarget:function(e){return e.relatedTarget===void 0?e.fromElement===e.srcElement?e.toElement:e.fromElement:e.relatedTarget},movementX:function(e){return"movementX"in e?e.movementX:(e!==Us&&(Us&&e.type==="mousemove"?(gd=e.screenX-Us.screenX,hd=e.screenY-Us.screenY):hd=gd=0,Us=e),gd)},movementY:function(e){return"movementY"in e?e.movementY:hd}}),zf=Bn(Pc),O2=It({},Pc,{dataTransfer:0}),B2=Bn(O2),z2=It({},Cl,{relatedTarget:0}),yd=Bn(z2),H2=It({},Ki,{animationName:0,elapsedTime:0,pseudoElement:0}),W2=Bn(H2),U2=It({},Ki,{clipboardData:function(e){return"clipboardData"in e?e.clipboardData:window.clipboardData}}),j2=Bn(U2),Y2=It({},Ki,{data:0}),Hf=Bn(Y2),V2={Esc:"Escape",Spacebar:" ",Left:"ArrowLeft",Up:"ArrowUp",Right:"ArrowRight",Down:"ArrowDown",Del:"Delete",Win:"OS",Menu:"ContextMenu",Apps:"ContextMenu",Scroll:"ScrollLock",MozPrintableKey:"Unidentified"},X2={8:"Backspace",9:"Tab",12:"Clear",13:"Enter",16:"Shift",17:"Control",18:"Alt",19:"Pause",20:"CapsLock",27:"Escape",32:" ",33:"PageUp",34:"PageDown",35:"End",36:"Home",37:"ArrowLeft",38:"ArrowUp",39:"ArrowRight",40:"ArrowDown",45:"Insert",46:"Delete",112:"F1",113:"F2",114:"F3",115:"F4",116:"F5",117:"F6",118:"F7",119:"F8",120:"F9",121:"F10",122:"F11",123:"F12",144:"NumLock",145:"ScrollLock",224:"Meta"},Q2={Alt:"altKey",Control:"ctrlKey",Meta:"metaKey",Shift:"shiftKey"};function G2(e){var t=this.nativeEvent;return t.getModifierState?t.getModifierState(e):(e=Q2[e])?!!t[e]:!1}function O_(){return G2}var q2=It({},Cl,{key:function(e){if(e.key){var t=V2[e.key]||e.key;if(t!=="Unidentified")return t}return e.type==="keypress"?(e=Ja(e),e===13?"Enter":String.fromCharCode(e)):e.type==="keydown"||e.type==="keyup"?X2[e.keyCode]||"Unidentified":""},code:0,location:0,ctrlKey:0,shiftKey:0,altKey:0,metaKey:0,repeat:0,locale:0,getModifierState:O_,charCode:function(e){return e.type==="keypress"?Ja(e):0},keyCode:function(e){return e.type==="keydown"||e.type==="keyup"?e.keyCode:0},which:function(e){return e.type==="keypress"?Ja(e):e.type==="keydown"||e.type==="keyup"?e.keyCode:0}}),K2=Bn(q2),J2=It({},Pc,{pointerId:0,width:0,height:0,pressure:0,tangentialPressure:0,tiltX:0,tiltY:0,twist:0,pointerType:0,isPrimary:0}),Wf=Bn(J2),Z2=It({},Cl,{touches:0,targetTouches:0,changedTouches:0,altKey:0,metaKey:0,ctrlKey:0,shiftKey:0,getModifierState:O_}),ex=Bn(Z2),tx=It({},Ki,{propertyName:0,elapsedTime:0,pseudoElement:0}),nx=Bn(tx),ox=It({},Pc,{deltaX:function(e){return"deltaX"in e?e.deltaX:"wheelDeltaX"in e?-e.wheelDeltaX:0},deltaY:function(e){return"deltaY"in e?e.deltaY:"wheelDeltaY"in e?-e.wheelDeltaY:"wheelDelta"in e?-e.wheelDelta:0},deltaZ:0,deltaMode:0}),rx=Bn(ox),ix=[9,13,27,32],B_=jo&&"CompositionEvent"in window,el=null;jo&&"documentMode"in document&&(el=document.documentMode);var sx=jo&&"TextEvent"in window&&!el,dh=jo&&(!B_||el&&8<el&&11>=el),Uf=" ",jf=!1;function _h(e,t){switch(e){case"keyup":return ix.indexOf(t.keyCode)!==-1;case"keydown":return t.keyCode!==229;case"keypress":case"mousedown":case"focusout":return!0;default:return!1}}function mh(e){return e=e.detail,typeof e=="object"&&"data"in e?e.data:null}var Ti=!1;function lx(e,t){switch(e){case"compositionend":return mh(t);case"keypress":return t.which!==32?null:(jf=!0,Uf);case"textInput":return e=t.data,e===Uf&&jf?null:e;default:return null}}function ax(e,t){if(Ti)return e==="compositionend"||!B_&&_h(e,t)?(e=uh(),Ka=$_=ar=null,Ti=!1,e):null;switch(e){case"paste":return null;case"keypress":if(!(t.ctrlKey||t.altKey||t.metaKey)||t.ctrlKey&&t.altKey){if(t.char&&1<t.char.length)return t.char;if(t.which)return String.fromCharCode(t.which)}return null;case"compositionend":return dh&&t.locale!=="ko"?null:t.data;default:return null}}var cx={color:!0,date:!0,datetime:!0,"datetime-local":!0,email:!0,month:!0,number:!0,password:!0,range:!0,search:!0,tel:!0,text:!0,time:!0,url:!0,week:!0};function Yf(e){var t=e&&e.nodeName&&e.nodeName.toLowerCase();return t==="input"?!!cx[e.type]:t==="textarea"}function ph(e,t,n,o){Yg(o),t=mc(t,"onChange"),0<t.length&&(n=new F_("onChange","change",null,n,o),e.push({event:n,listeners:t}))}var tl=null,ml=null;function ux(e){Ch(e,0)}function Nc(e){var t=Pi(e);if(Og(t))return e}function dx(e,t){if(e==="change")return t}var fh=!1;jo&&(jo?(Ba="oninput"in document,Ba||(xd=document.createElement("div"),xd.setAttribute("oninput","return;"),Ba=typeof xd.oninput=="function"),Oa=Ba):Oa=!1,fh=Oa&&(!document.documentMode||9<document.documentMode));var Oa,Ba,xd;function Vf(){tl&&(tl.detachEvent("onpropertychange",gh),ml=tl=null)}function gh(e){if(e.propertyName==="value"&&Nc(ml)){var t=[];ph(t,ml,e,P_(e)),Gg(ux,t)}}function _x(e,t,n){e==="focusin"?(Vf(),tl=t,ml=n,tl.attachEvent("onpropertychange",gh)):e==="focusout"&&Vf()}function mx(e){if(e==="selectionchange"||e==="keyup"||e==="keydown")return Nc(ml)}function px(e,t){if(e==="click")return Nc(t)}function fx(e,t){if(e==="input"||e==="change")return Nc(t)}function gx(e,t){return e===t&&(e!==0||1/e===1/t)||e!==e&&t!==t}var _o=typeof Object.is=="function"?Object.is:gx;function pl(e,t){if(_o(e,t))return!0;if(typeof e!="object"||e===null||typeof t!="object"||t===null)return!1;var n=Object.keys(e),o=Object.keys(t);if(n.length!==o.length)return!1;for(o=0;o<n.length;o++){var r=n[o];if(!Id.call(t,r)||!_o(e[r],t[r]))return!1}return!0}function Xf(e){for(;e&&e.firstChild;)e=e.firstChild;return e}function Qf(e,t){var n=Xf(e);e=0;for(var o;n;){if(n.nodeType===3){if(o=e+n.textContent.length,e<=t&&o>=t)return{node:n,offset:t-e};e=o}e:{for(;n;){if(n.nextSibling){n=n.nextSibling;break e}n=n.parentNode}n=void 0}n=Xf(n)}}function hh(e,t){return e&&t?e===t?!0:e&&e.nodeType===3?!1:t&&t.nodeType===3?hh(e,t.parentNode):"contains"in e?e.contains(t):e.compareDocumentPosition?!!(e.compareDocumentPosition(t)&16):!1:!1}function yh(){for(var e=window,t=sc();t instanceof e.HTMLIFrameElement;){try{var n=typeof t.contentWindow.location.href=="string"}catch{n=!1}if(n)e=t.contentWindow;else break;t=sc(e.document)}return t}function z_(e){var t=e&&e.nodeName&&e.nodeName.toLowerCase();return t&&(t==="input"&&(e.type==="text"||e.type==="search"||e.type==="tel"||e.type==="url"||e.type==="password")||t==="textarea"||e.contentEditable==="true")}function hx(e){var t=yh(),n=e.focusedElem,o=e.selectionRange;if(t!==n&&n&&n.ownerDocument&&hh(n.ownerDocument.documentElement,n)){if(o!==null&&z_(n)){if(t=o.start,e=o.end,e===void 0&&(e=t),"selectionStart"in n)n.selectionStart=t,n.selectionEnd=Math.min(e,n.value.length);else if(e=(t=n.ownerDocument||document)&&t.defaultView||window,e.getSelection){e=e.getSelection();var r=n.textContent.length,i=Math.min(o.start,r);o=o.end===void 0?i:Math.min(o.end,r),!e.extend&&i>o&&(r=o,o=i,i=r),r=Qf(n,i);var s=Qf(n,o);r&&s&&(e.rangeCount!==1||e.anchorNode!==r.node||e.anchorOffset!==r.offset||e.focusNode!==s.node||e.focusOffset!==s.offset)&&(t=t.createRange(),t.setStart(r.node,r.offset),e.removeAllRanges(),i>o?(e.addRange(t),e.extend(s.node,s.offset)):(t.setEnd(s.node,s.offset),e.addRange(t)))}}for(t=[],e=n;e=e.parentNode;)e.nodeType===1&&t.push({element:e,left:e.scrollLeft,top:e.scrollTop});for(typeof n.focus=="function"&&n.focus(),n=0;n<t.length;n++)e=t[n],e.element.scrollLeft=e.left,e.element.scrollTop=e.top}}var yx=jo&&"documentMode"in document&&11>=document.documentMode,Li=null,Jd=null,nl=null,Zd=!1;function Gf(e,t,n){var o=n.window===n?n.document:n.nodeType===9?n:n.ownerDocument;Zd||Li==null||Li!==sc(o)||(o=Li,"selectionStart"in o&&z_(o)?o={start:o.selectionStart,end:o.selectionEnd}:(o=(o.ownerDocument&&o.ownerDocument.defaultView||window).getSelection(),o={anchorNode:o.anchorNode,anchorOffset:o.anchorOffset,focusNode:o.focusNode,focusOffset:o.focusOffset}),nl&&pl(nl,o)||(nl=o,o=mc(Jd,"onSelect"),0<o.length&&(t=new F_("onSelect","select",null,t,n),e.push({event:t,listeners:o}),t.target=Li)))}function za(e,t){var n={};return n[e.toLowerCase()]=t.toLowerCase(),n["Webkit"+e]="webkit"+t,n["Moz"+e]="moz"+t,n}var Ri={animationend:za("Animation","AnimationEnd"),animationiteration:za("Animation","AnimationIteration"),animationstart:za("Animation","AnimationStart"),transitionend:za("Transition","TransitionEnd")},vd={},xh={};jo&&(xh=document.createElement("div").style,"AnimationEvent"in window||(delete Ri.animationend.animation,delete Ri.animationiteration.animation,delete Ri.animationstart.animation),"TransitionEvent"in window||delete Ri.transitionend.transition);function Ic(e){if(vd[e])return vd[e];if(!Ri[e])return e;var t=Ri[e],n;for(n in t)if(t.hasOwnProperty(n)&&n in xh)return vd[e]=t[n];return e}var vh=Ic("animationend"),wh=Ic("animationiteration"),bh=Ic("animationstart"),kh=Ic("transitionend"),Sh=new Map,qf="abort auxClick cancel canPlay canPlayThrough click close contextMenu copy cut drag dragEnd dragEnter dragExit dragLeave dragOver dragStart drop durationChange emptied encrypted ended error gotPointerCapture input invalid keyDown keyPress keyUp load loadedData loadedMetadata loadStart lostPointerCapture mouseDown mouseMove mouseOut mouseOver mouseUp paste pause play playing pointerCancel pointerDown pointerMove pointerOut pointerOver pointerUp progress rateChange reset resize seeked seeking stalled submit suspend timeUpdate touchCancel touchEnd touchStart volumeChange scroll toggle touchMove waiting wheel".split(" ");function vr(e,t){Sh.set(e,t),oi(t,[e])}for(Ha=0;Ha<qf.length;Ha++)Wa=qf[Ha],Kf=Wa.toLowerCase(),Jf=Wa[0].toUpperCase()+Wa.slice(1),vr(Kf,"on"+Jf);var Wa,Kf,Jf,Ha;vr(vh,"onAnimationEnd");vr(wh,"onAnimationIteration");vr(bh,"onAnimationStart");vr("dblclick","onDoubleClick");vr("focusin","onFocus");vr("focusout","onBlur");vr(kh,"onTransitionEnd");ji("onMouseEnter",["mouseout","mouseover"]);ji("onMouseLeave",["mouseout","mouseover"]);ji("onPointerEnter",["pointerout","pointerover"]);ji("onPointerLeave",["pointerout","pointerover"]);oi("onChange","change click focusin focusout input keydown keyup selectionchange".split(" "));oi("onSelect","focusout contextmenu dragend focusin keydown keyup mousedown mouseup selectionchange".split(" "));oi("onBeforeInput",["compositionend","keypress","textInput","paste"]);oi("onCompositionEnd","compositionend focusout keydown keypress keyup mousedown".split(" "));oi("onCompositionStart","compositionstart focusout keydown keypress keyup mousedown".split(" "));oi("onCompositionUpdate","compositionupdate focusout keydown keypress keyup mousedown".split(" "));var Ks="abort canplay canplaythrough durationchange emptied encrypted ended error loadeddata loadedmetadata loadstart pause play playing progress ratechange resize seeked seeking stalled suspend timeupdate volumechange waiting".split(" "),xx=new Set("cancel close invalid load scroll toggle".split(" ").concat(Ks));function Zf(e,t,n){var o=e.type||"unknown-event";e.currentTarget=n,x2(o,t,void 0,e),e.currentTarget=null}function Ch(e,t){t=(t&4)!==0;for(var n=0;n<e.length;n++){var o=e[n],r=o.event;o=o.listeners;e:{var i=void 0;if(t)for(var s=o.length-1;0<=s;s--){var l=o[s],a=l.instance,c=l.currentTarget;if(l=l.listener,a!==i&&r.isPropagationStopped())break e;Zf(r,l,c),i=a}else for(s=0;s<o.length;s++){if(l=o[s],a=l.instance,c=l.currentTarget,l=l.listener,a!==i&&r.isPropagationStopped())break e;Zf(r,l,c),i=a}}}if(ac)throw e=Qd,ac=!1,Qd=null,e}function Mt(e,t){var n=t[r_];n===void 0&&(n=t[r_]=new Set);var o=e+"__bubble";n.has(o)||(Eh(t,e,2,!1),n.add(o))}function wd(e,t,n){var o=0;t&&(o|=4),Eh(n,e,o,t)}var Ua="_reactListening"+Math.random().toString(36).slice(2);function fl(e){if(!e[Ua]){e[Ua]=!0,Ig.forEach(function(n){n!=="selectionchange"&&(xx.has(n)||wd(n,!1,e),wd(n,!0,e))});var t=e.nodeType===9?e:e.ownerDocument;t===null||t[Ua]||(t[Ua]=!0,wd("selectionchange",!1,t))}}function Eh(e,t,n,o){switch(ch(t)){case 1:var r=A2;break;case 4:r=$2;break;default:r=A_}n=r.bind(null,t,n,e),r=void 0,!Xd||t!=="touchstart"&&t!=="touchmove"&&t!=="wheel"||(r=!0),o?r!==void 0?e.addEventListener(t,n,{capture:!0,passive:r}):e.addEventListener(t,n,!0):r!==void 0?e.addEventListener(t,n,{passive:r}):e.addEventListener(t,n,!1)}function bd(e,t,n,o,r){var i=o;if((t&1)===0&&(t&2)===0&&o!==null)e:for(;;){if(o===null)return;var s=o.tag;if(s===3||s===4){var l=o.stateNode.containerInfo;if(l===r||l.nodeType===8&&l.parentNode===r)break;if(s===4)for(s=o.return;s!==null;){var a=s.tag;if((a===3||a===4)&&(a=s.stateNode.containerInfo,a===r||a.nodeType===8&&a.parentNode===r))return;s=s.return}for(;l!==null;){if(s=Xr(l),s===null)return;if(a=s.tag,a===5||a===6){o=i=s;continue e}l=l.parentNode}}o=o.return}Gg(function(){var c=i,d=P_(n),m=[];e:{var f=Sh.get(e);if(f!==void 0){var k=F_,w=e;switch(e){case"keypress":if(Ja(n)===0)break e;case"keydown":case"keyup":k=K2;break;case"focusin":w="focus",k=yd;break;case"focusout":w="blur",k=yd;break;case"beforeblur":case"afterblur":k=yd;break;case"click":if(n.button===2)break e;case"auxclick":case"dblclick":case"mousedown":case"mousemove":case"mouseup":case"mouseout":case"mouseover":case"contextmenu":k=zf;break;case"drag":case"dragend":case"dragenter":case"dragexit":case"dragleave":case"dragover":case"dragstart":case"drop":k=B2;break;case"touchcancel":case"touchend":case"touchmove":case"touchstart":k=ex;break;case vh:case wh:case bh:k=W2;break;case kh:k=nx;break;case"scroll":k=F2;break;case"wheel":k=rx;break;case"copy":case"cut":case"paste":k=j2;break;case"gotpointercapture":case"lostpointercapture":case"pointercancel":case"pointerdown":case"pointermove":case"pointerout":case"pointerover":case"pointerup":k=Wf}var C=(t&4)!==0,S=!C&&e==="scroll",g=C?f!==null?f+"Capture":null:f;C=[];for(var x=c,h;x!==null;){h=x;var E=h.stateNode;if(h.tag===5&&E!==null&&(h=E,g!==null&&(E=cl(x,g),E!=null&&C.push(gl(x,E,h)))),S)break;x=x.return}0<C.length&&(f=new k(f,w,null,n,d),m.push({event:f,listeners:C}))}}if((t&7)===0){e:{if(f=e==="mouseover"||e==="pointerover",k=e==="mouseout"||e==="pointerout",f&&n!==Yd&&(w=n.relatedTarget||n.fromElement)&&(Xr(w)||w[Yo]))break e;if((k||f)&&(f=d.window===d?d:(f=d.ownerDocument)?f.defaultView||f.parentWindow:window,k?(w=n.relatedTarget||n.toElement,k=c,w=w?Xr(w):null,w!==null&&(S=ri(w),w!==S||w.tag!==5&&w.tag!==6)&&(w=null)):(k=null,w=c),k!==w)){if(C=zf,E="onMouseLeave",g="onMouseEnter",x="mouse",(e==="pointerout"||e==="pointerover")&&(C=Wf,E="onPointerLeave",g="onPointerEnter",x="pointer"),S=k==null?f:Pi(k),h=w==null?f:Pi(w),f=new C(E,x+"leave",k,n,d),f.target=S,f.relatedTarget=h,E=null,Xr(d)===c&&(C=new C(g,x+"enter",w,n,d),C.target=h,C.relatedTarget=S,E=C),S=E,k&&w)t:{for(C=k,g=w,x=0,h=C;h;h=Ci(h))x++;for(h=0,E=g;E;E=Ci(E))h++;for(;0<x-h;)C=Ci(C),x--;for(;0<h-x;)g=Ci(g),h--;for(;x--;){if(C===g||g!==null&&C===g.alternate)break t;C=Ci(C),g=Ci(g)}C=null}else C=null;k!==null&&eg(m,f,k,C,!1),w!==null&&S!==null&&eg(m,S,w,C,!0)}}e:{if(f=c?Pi(c):window,k=f.nodeName&&f.nodeName.toLowerCase(),k==="select"||k==="input"&&f.type==="file")var O=dx;else if(Yf(f))if(fh)O=fx;else{O=mx;var j=_x}else(k=f.nodeName)&&k.toLowerCase()==="input"&&(f.type==="checkbox"||f.type==="radio")&&(O=px);if(O&&(O=O(e,c))){ph(m,O,n,d);break e}j&&j(e,f,c),e==="focusout"&&(j=f._wrapperState)&&j.controlled&&f.type==="number"&&zd(f,"number",f.value)}switch(j=c?Pi(c):window,e){case"focusin":(Yf(j)||j.contentEditable==="true")&&(Li=j,Jd=c,nl=null);break;case"focusout":nl=Jd=Li=null;break;case"mousedown":Zd=!0;break;case"contextmenu":case"mouseup":case"dragend":Zd=!1,Gf(m,n,d);break;case"selectionchange":if(yx)break;case"keydown":case"keyup":Gf(m,n,d)}var A;if(B_)e:{switch(e){case"compositionstart":var Y="onCompositionStart";break e;case"compositionend":Y="onCompositionEnd";break e;case"compositionupdate":Y="onCompositionUpdate";break e}Y=void 0}else Ti?_h(e,n)&&(Y="onCompositionEnd"):e==="keydown"&&n.keyCode===229&&(Y="onCompositionStart");Y&&(dh&&n.locale!=="ko"&&(Ti||Y!=="onCompositionStart"?Y==="onCompositionEnd"&&Ti&&(A=uh()):(ar=d,$_="value"in ar?ar.value:ar.textContent,Ti=!0)),j=mc(c,Y),0<j.length&&(Y=new Hf(Y,e,null,n,d),m.push({event:Y,listeners:j}),A?Y.data=A:(A=mh(n),A!==null&&(Y.data=A)))),(A=sx?lx(e,n):ax(e,n))&&(c=mc(c,"onBeforeInput"),0<c.length&&(d=new Hf("onBeforeInput","beforeinput",null,n,d),m.push({event:d,listeners:c}),d.data=A))}Ch(m,t)})}function gl(e,t,n){return{instance:e,listener:t,currentTarget:n}}function mc(e,t){for(var n=t+"Capture",o=[];e!==null;){var r=e,i=r.stateNode;r.tag===5&&i!==null&&(r=i,i=cl(e,n),i!=null&&o.unshift(gl(e,i,r)),i=cl(e,t),i!=null&&o.push(gl(e,i,r))),e=e.return}return o}function Ci(e){if(e===null)return null;do e=e.return;while(e&&e.tag!==5);return e||null}function eg(e,t,n,o,r){for(var i=t._reactName,s=[];n!==null&&n!==o;){var l=n,a=l.alternate,c=l.stateNode;if(a!==null&&a===o)break;l.tag===5&&c!==null&&(l=c,r?(a=cl(n,i),a!=null&&s.unshift(gl(n,a,l))):r||(a=cl(n,i),a!=null&&s.push(gl(n,a,l)))),n=n.return}s.length!==0&&e.push({event:t,listeners:s})}var vx=/\r\n?/g,wx=/\u0000|\uFFFD/g;function tg(e){return(typeof e=="string"?e:""+e).replace(vx,`
`).replace(wx,"")}function ja(e,t,n){if(t=tg(t),tg(e)!==t&&n)throw Error(Q(425))}function pc(){}var e_=null,t_=null;function n_(e,t){return e==="textarea"||e==="noscript"||typeof t.children=="string"||typeof t.children=="number"||typeof t.dangerouslySetInnerHTML=="object"&&t.dangerouslySetInnerHTML!==null&&t.dangerouslySetInnerHTML.__html!=null}var o_=typeof setTimeout=="function"?setTimeout:void 0,bx=typeof clearTimeout=="function"?clearTimeout:void 0,ng=typeof Promise=="function"?Promise:void 0,kx=typeof queueMicrotask=="function"?queueMicrotask:typeof ng<"u"?function(e){return ng.resolve(null).then(e).catch(Sx)}:o_;function Sx(e){setTimeout(function(){throw e})}function kd(e,t){var n=t,o=0;do{var r=n.nextSibling;if(e.removeChild(n),r&&r.nodeType===8)if(n=r.data,n==="/$"){if(o===0){e.removeChild(r),_l(t);return}o--}else n!=="$"&&n!=="$?"&&n!=="$!"||o++;n=r}while(n);_l(t)}function mr(e){for(;e!=null;e=e.nextSibling){var t=e.nodeType;if(t===1||t===3)break;if(t===8){if(t=e.data,t==="$"||t==="$!"||t==="$?")break;if(t==="/$")return null}}return e}function og(e){e=e.previousSibling;for(var t=0;e;){if(e.nodeType===8){var n=e.data;if(n==="$"||n==="$!"||n==="$?"){if(t===0)return e;t--}else n==="/$"&&t++}e=e.previousSibling}return null}var Ji=Math.random().toString(36).slice(2),Co="__reactFiber$"+Ji,hl="__reactProps$"+Ji,Yo="__reactContainer$"+Ji,r_="__reactEvents$"+Ji,Cx="__reactListeners$"+Ji,Ex="__reactHandles$"+Ji;function Xr(e){var t=e[Co];if(t)return t;for(var n=e.parentNode;n;){if(t=n[Yo]||n[Co]){if(n=t.alternate,t.child!==null||n!==null&&n.child!==null)for(e=og(e);e!==null;){if(n=e[Co])return n;e=og(e)}return t}e=n,n=e.parentNode}return null}function El(e){return e=e[Co]||e[Yo],!e||e.tag!==5&&e.tag!==6&&e.tag!==13&&e.tag!==3?null:e}function Pi(e){if(e.tag===5||e.tag===6)return e.stateNode;throw Error(Q(33))}function Dc(e){return e[hl]||null}var i_=[],Ni=-1;function wr(e){return{current:e}}function Tt(e){0>Ni||(e.current=i_[Ni],i_[Ni]=null,Ni--)}function St(e,t){Ni++,i_[Ni]=e.current,e.current=t}var xr={},pn=wr(xr),En=wr(!1),Jr=xr;function Yi(e,t){var n=e.type.contextTypes;if(!n)return xr;var o=e.stateNode;if(o&&o.__reactInternalMemoizedUnmaskedChildContext===t)return o.__reactInternalMemoizedMaskedChildContext;var r={},i;for(i in n)r[i]=t[i];return o&&(e=e.stateNode,e.__reactInternalMemoizedUnmaskedChildContext=t,e.__reactInternalMemoizedMaskedChildContext=r),r}function Mn(e){return e=e.childContextTypes,e!=null}function fc(){Tt(En),Tt(pn)}function rg(e,t,n){if(pn.current!==xr)throw Error(Q(168));St(pn,t),St(En,n)}function Mh(e,t,n){var o=e.stateNode;if(t=t.childContextTypes,typeof o.getChildContext!="function")return n;o=o.getChildContext();for(var r in o)if(!(r in t))throw Error(Q(108,_2(e)||"Unknown",r));return It({},n,o)}function gc(e){return e=(e=e.stateNode)&&e.__reactInternalMemoizedMergedChildContext||xr,Jr=pn.current,St(pn,e),St(En,En.current),!0}function ig(e,t,n){var o=e.stateNode;if(!o)throw Error(Q(169));n?(e=Mh(e,t,Jr),o.__reactInternalMemoizedMergedChildContext=e,Tt(En),Tt(pn),St(pn,e)):Tt(En),St(En,n)}var zo=null,Ac=!1,Sd=!1;function Th(e){zo===null?zo=[e]:zo.push(e)}function Mx(e){Ac=!0,Th(e)}function br(){if(!Sd&&zo!==null){Sd=!0;var e=0,t=_t;try{var n=zo;for(_t=1;e<n.length;e++){var o=n[e];do o=o(!0);while(o!==null)}zo=null,Ac=!1}catch(r){throw zo!==null&&(zo=zo.slice(e+1)),Zg(N_,br),r}finally{_t=t,Sd=!1}}return null}var Ii=[],Di=0,hc=null,yc=0,Vn=[],Xn=0,Zr=null,Ho=1,Wo="";function Yr(e,t){Ii[Di++]=yc,Ii[Di++]=hc,hc=e,yc=t}function Lh(e,t,n){Vn[Xn++]=Ho,Vn[Xn++]=Wo,Vn[Xn++]=Zr,Zr=e;var o=Ho;e=Wo;var r=32-co(o)-1;o&=~(1<<r),n+=1;var i=32-co(t)+r;if(30<i){var s=r-r%5;i=(o&(1<<s)-1).toString(32),o>>=s,r-=s,Ho=1<<32-co(t)+r|n<<r|o,Wo=i+e}else Ho=1<<i|n<<r|o,Wo=e}function H_(e){e.return!==null&&(Yr(e,1),Lh(e,1,0))}function W_(e){for(;e===hc;)hc=Ii[--Di],Ii[Di]=null,yc=Ii[--Di],Ii[Di]=null;for(;e===Zr;)Zr=Vn[--Xn],Vn[Xn]=null,Wo=Vn[--Xn],Vn[Xn]=null,Ho=Vn[--Xn],Vn[Xn]=null}var Fn=null,$n=null,Rt=!1,ao=null;function Rh(e,t){var n=Qn(5,null,null,0);n.elementType="DELETED",n.stateNode=t,n.return=e,t=e.deletions,t===null?(e.deletions=[n],e.flags|=16):t.push(n)}function sg(e,t){switch(e.tag){case 5:var n=e.type;return t=t.nodeType!==1||n.toLowerCase()!==t.nodeName.toLowerCase()?null:t,t!==null?(e.stateNode=t,Fn=e,$n=mr(t.firstChild),!0):!1;case 6:return t=e.pendingProps===""||t.nodeType!==3?null:t,t!==null?(e.stateNode=t,Fn=e,$n=null,!0):!1;case 13:return t=t.nodeType!==8?null:t,t!==null?(n=Zr!==null?{id:Ho,overflow:Wo}:null,e.memoizedState={dehydrated:t,treeContext:n,retryLane:1073741824},n=Qn(18,null,null,0),n.stateNode=t,n.return=e,e.child=n,Fn=e,$n=null,!0):!1;default:return!1}}function s_(e){return(e.mode&1)!==0&&(e.flags&128)===0}function l_(e){if(Rt){var t=$n;if(t){var n=t;if(!sg(e,t)){if(s_(e))throw Error(Q(418));t=mr(n.nextSibling);var o=Fn;t&&sg(e,t)?Rh(o,n):(e.flags=e.flags&-4097|2,Rt=!1,Fn=e)}}else{if(s_(e))throw Error(Q(418));e.flags=e.flags&-4097|2,Rt=!1,Fn=e}}}function lg(e){for(e=e.return;e!==null&&e.tag!==5&&e.tag!==3&&e.tag!==13;)e=e.return;Fn=e}function Ya(e){if(e!==Fn)return!1;if(!Rt)return lg(e),Rt=!0,!1;var t;if((t=e.tag!==3)&&!(t=e.tag!==5)&&(t=e.type,t=t!=="head"&&t!=="body"&&!n_(e.type,e.memoizedProps)),t&&(t=$n)){if(s_(e))throw Ph(),Error(Q(418));for(;t;)Rh(e,t),t=mr(t.nextSibling)}if(lg(e),e.tag===13){if(e=e.memoizedState,e=e!==null?e.dehydrated:null,!e)throw Error(Q(317));e:{for(e=e.nextSibling,t=0;e;){if(e.nodeType===8){var n=e.data;if(n==="/$"){if(t===0){$n=mr(e.nextSibling);break e}t--}else n!=="$"&&n!=="$!"&&n!=="$?"||t++}e=e.nextSibling}$n=null}}else $n=Fn?mr(e.stateNode.nextSibling):null;return!0}function Ph(){for(var e=$n;e;)e=mr(e.nextSibling)}function Vi(){$n=Fn=null,Rt=!1}function U_(e){ao===null?ao=[e]:ao.push(e)}var Tx=Qo.ReactCurrentBatchConfig;function js(e,t,n){if(e=n.ref,e!==null&&typeof e!="function"&&typeof e!="object"){if(n._owner){if(n=n._owner,n){if(n.tag!==1)throw Error(Q(309));var o=n.stateNode}if(!o)throw Error(Q(147,e));var r=o,i=""+e;return t!==null&&t.ref!==null&&typeof t.ref=="function"&&t.ref._stringRef===i?t.ref:(t=function(s){var l=r.refs;s===null?delete l[i]:l[i]=s},t._stringRef=i,t)}if(typeof e!="string")throw Error(Q(284));if(!n._owner)throw Error(Q(290,e))}return e}function Va(e,t){throw e=Object.prototype.toString.call(t),Error(Q(31,e==="[object Object]"?"object with keys {"+Object.keys(t).join(", ")+"}":e))}function ag(e){var t=e._init;return t(e._payload)}function Nh(e){function t(g,x){if(e){var h=g.deletions;h===null?(g.deletions=[x],g.flags|=16):h.push(x)}}function n(g,x){if(!e)return null;for(;x!==null;)t(g,x),x=x.sibling;return null}function o(g,x){for(g=new Map;x!==null;)x.key!==null?g.set(x.key,x):g.set(x.index,x),x=x.sibling;return g}function r(g,x){return g=hr(g,x),g.index=0,g.sibling=null,g}function i(g,x,h){return g.index=h,e?(h=g.alternate,h!==null?(h=h.index,h<x?(g.flags|=2,x):h):(g.flags|=2,x)):(g.flags|=1048576,x)}function s(g){return e&&g.alternate===null&&(g.flags|=2),g}function l(g,x,h,E){return x===null||x.tag!==6?(x=Pd(h,g.mode,E),x.return=g,x):(x=r(x,h),x.return=g,x)}function a(g,x,h,E){var O=h.type;return O===Mi?d(g,x,h.props.children,E,h.key):x!==null&&(x.elementType===O||typeof O=="object"&&O!==null&&O.$$typeof===rr&&ag(O)===x.type)?(E=r(x,h.props),E.ref=js(g,x,h),E.return=g,E):(E=ic(h.type,h.key,h.props,null,g.mode,E),E.ref=js(g,x,h),E.return=g,E)}function c(g,x,h,E){return x===null||x.tag!==4||x.stateNode.containerInfo!==h.containerInfo||x.stateNode.implementation!==h.implementation?(x=Nd(h,g.mode,E),x.return=g,x):(x=r(x,h.children||[]),x.return=g,x)}function d(g,x,h,E,O){return x===null||x.tag!==7?(x=Kr(h,g.mode,E,O),x.return=g,x):(x=r(x,h),x.return=g,x)}function m(g,x,h){if(typeof x=="string"&&x!==""||typeof x=="number")return x=Pd(""+x,g.mode,h),x.return=g,x;if(typeof x=="object"&&x!==null){switch(x.$$typeof){case Pa:return h=ic(x.type,x.key,x.props,null,g.mode,h),h.ref=js(g,null,x),h.return=g,h;case Ei:return x=Nd(x,g.mode,h),x.return=g,x;case rr:var E=x._init;return m(g,E(x._payload),h)}if(Gs(x)||zs(x))return x=Kr(x,g.mode,h,null),x.return=g,x;Va(g,x)}return null}function f(g,x,h,E){var O=x!==null?x.key:null;if(typeof h=="string"&&h!==""||typeof h=="number")return O!==null?null:l(g,x,""+h,E);if(typeof h=="object"&&h!==null){switch(h.$$typeof){case Pa:return h.key===O?a(g,x,h,E):null;case Ei:return h.key===O?c(g,x,h,E):null;case rr:return O=h._init,f(g,x,O(h._payload),E)}if(Gs(h)||zs(h))return O!==null?null:d(g,x,h,E,null);Va(g,h)}return null}function k(g,x,h,E,O){if(typeof E=="string"&&E!==""||typeof E=="number")return g=g.get(h)||null,l(x,g,""+E,O);if(typeof E=="object"&&E!==null){switch(E.$$typeof){case Pa:return g=g.get(E.key===null?h:E.key)||null,a(x,g,E,O);case Ei:return g=g.get(E.key===null?h:E.key)||null,c(x,g,E,O);case rr:var j=E._init;return k(g,x,h,j(E._payload),O)}if(Gs(E)||zs(E))return g=g.get(h)||null,d(x,g,E,O,null);Va(x,E)}return null}function w(g,x,h,E){for(var O=null,j=null,A=x,Y=x=0,K=null;A!==null&&Y<h.length;Y++){A.index>Y?(K=A,A=null):K=A.sibling;var q=f(g,A,h[Y],E);if(q===null){A===null&&(A=K);break}e&&A&&q.alternate===null&&t(g,A),x=i(q,x,Y),j===null?O=q:j.sibling=q,j=q,A=K}if(Y===h.length)return n(g,A),Rt&&Yr(g,Y),O;if(A===null){for(;Y<h.length;Y++)A=m(g,h[Y],E),A!==null&&(x=i(A,x,Y),j===null?O=A:j.sibling=A,j=A);return Rt&&Yr(g,Y),O}for(A=o(g,A);Y<h.length;Y++)K=k(A,g,Y,h[Y],E),K!==null&&(e&&K.alternate!==null&&A.delete(K.key===null?Y:K.key),x=i(K,x,Y),j===null?O=K:j.sibling=K,j=K);return e&&A.forEach(function(Z){return t(g,Z)}),Rt&&Yr(g,Y),O}function C(g,x,h,E){var O=zs(h);if(typeof O!="function")throw Error(Q(150));if(h=O.call(h),h==null)throw Error(Q(151));for(var j=O=null,A=x,Y=x=0,K=null,q=h.next();A!==null&&!q.done;Y++,q=h.next()){A.index>Y?(K=A,A=null):K=A.sibling;var Z=f(g,A,q.value,E);if(Z===null){A===null&&(A=K);break}e&&A&&Z.alternate===null&&t(g,A),x=i(Z,x,Y),j===null?O=Z:j.sibling=Z,j=Z,A=K}if(q.done)return n(g,A),Rt&&Yr(g,Y),O;if(A===null){for(;!q.done;Y++,q=h.next())q=m(g,q.value,E),q!==null&&(x=i(q,x,Y),j===null?O=q:j.sibling=q,j=q);return Rt&&Yr(g,Y),O}for(A=o(g,A);!q.done;Y++,q=h.next())q=k(A,g,Y,q.value,E),q!==null&&(e&&q.alternate!==null&&A.delete(q.key===null?Y:q.key),x=i(q,x,Y),j===null?O=q:j.sibling=q,j=q);return e&&A.forEach(function(Ie){return t(g,Ie)}),Rt&&Yr(g,Y),O}function S(g,x,h,E){if(typeof h=="object"&&h!==null&&h.type===Mi&&h.key===null&&(h=h.props.children),typeof h=="object"&&h!==null){switch(h.$$typeof){case Pa:e:{for(var O=h.key,j=x;j!==null;){if(j.key===O){if(O=h.type,O===Mi){if(j.tag===7){n(g,j.sibling),x=r(j,h.props.children),x.return=g,g=x;break e}}else if(j.elementType===O||typeof O=="object"&&O!==null&&O.$$typeof===rr&&ag(O)===j.type){n(g,j.sibling),x=r(j,h.props),x.ref=js(g,j,h),x.return=g,g=x;break e}n(g,j);break}else t(g,j);j=j.sibling}h.type===Mi?(x=Kr(h.props.children,g.mode,E,h.key),x.return=g,g=x):(E=ic(h.type,h.key,h.props,null,g.mode,E),E.ref=js(g,x,h),E.return=g,g=E)}return s(g);case Ei:e:{for(j=h.key;x!==null;){if(x.key===j)if(x.tag===4&&x.stateNode.containerInfo===h.containerInfo&&x.stateNode.implementation===h.implementation){n(g,x.sibling),x=r(x,h.children||[]),x.return=g,g=x;break e}else{n(g,x);break}else t(g,x);x=x.sibling}x=Nd(h,g.mode,E),x.return=g,g=x}return s(g);case rr:return j=h._init,S(g,x,j(h._payload),E)}if(Gs(h))return w(g,x,h,E);if(zs(h))return C(g,x,h,E);Va(g,h)}return typeof h=="string"&&h!==""||typeof h=="number"?(h=""+h,x!==null&&x.tag===6?(n(g,x.sibling),x=r(x,h),x.return=g,g=x):(n(g,x),x=Pd(h,g.mode,E),x.return=g,g=x),s(g)):n(g,x)}return S}var Xi=Nh(!0),Ih=Nh(!1),xc=wr(null),vc=null,Ai=null,j_=null;function Y_(){j_=Ai=vc=null}function V_(e){var t=xc.current;Tt(xc),e._currentValue=t}function a_(e,t,n){for(;e!==null;){var o=e.alternate;if((e.childLanes&t)!==t?(e.childLanes|=t,o!==null&&(o.childLanes|=t)):o!==null&&(o.childLanes&t)!==t&&(o.childLanes|=t),e===n)break;e=e.return}}function Wi(e,t){vc=e,j_=Ai=null,e=e.dependencies,e!==null&&e.firstContext!==null&&((e.lanes&t)!==0&&(Cn=!0),e.firstContext=null)}function qn(e){var t=e._currentValue;if(j_!==e)if(e={context:e,memoizedValue:t,next:null},Ai===null){if(vc===null)throw Error(Q(308));Ai=e,vc.dependencies={lanes:0,firstContext:e}}else Ai=Ai.next=e;return t}var Qr=null;function X_(e){Qr===null?Qr=[e]:Qr.push(e)}function Dh(e,t,n,o){var r=t.interleaved;return r===null?(n.next=n,X_(t)):(n.next=r.next,r.next=n),t.interleaved=n,Vo(e,o)}function Vo(e,t){e.lanes|=t;var n=e.alternate;for(n!==null&&(n.lanes|=t),n=e,e=e.return;e!==null;)e.childLanes|=t,n=e.alternate,n!==null&&(n.childLanes|=t),n=e,e=e.return;return n.tag===3?n.stateNode:null}var ir=!1;function Q_(e){e.updateQueue={baseState:e.memoizedState,firstBaseUpdate:null,lastBaseUpdate:null,shared:{pending:null,interleaved:null,lanes:0},effects:null}}function Ah(e,t){e=e.updateQueue,t.updateQueue===e&&(t.updateQueue={baseState:e.baseState,firstBaseUpdate:e.firstBaseUpdate,lastBaseUpdate:e.lastBaseUpdate,shared:e.shared,effects:e.effects})}function Uo(e,t){return{eventTime:e,lane:t,tag:0,payload:null,callback:null,next:null}}function pr(e,t,n){var o=e.updateQueue;if(o===null)return null;if(o=o.shared,(ot&2)!==0){var r=o.pending;return r===null?t.next=t:(t.next=r.next,r.next=t),o.pending=t,Vo(e,n)}return r=o.interleaved,r===null?(t.next=t,X_(o)):(t.next=r.next,r.next=t),o.interleaved=t,Vo(e,n)}function Za(e,t,n){if(t=t.updateQueue,t!==null&&(t=t.shared,(n&4194240)!==0)){var o=t.lanes;o&=e.pendingLanes,n|=o,t.lanes=n,I_(e,n)}}function cg(e,t){var n=e.updateQueue,o=e.alternate;if(o!==null&&(o=o.updateQueue,n===o)){var r=null,i=null;if(n=n.firstBaseUpdate,n!==null){do{var s={eventTime:n.eventTime,lane:n.lane,tag:n.tag,payload:n.payload,callback:n.callback,next:null};i===null?r=i=s:i=i.next=s,n=n.next}while(n!==null);i===null?r=i=t:i=i.next=t}else r=i=t;n={baseState:o.baseState,firstBaseUpdate:r,lastBaseUpdate:i,shared:o.shared,effects:o.effects},e.updateQueue=n;return}e=n.lastBaseUpdate,e===null?n.firstBaseUpdate=t:e.next=t,n.lastBaseUpdate=t}function wc(e,t,n,o){var r=e.updateQueue;ir=!1;var i=r.firstBaseUpdate,s=r.lastBaseUpdate,l=r.shared.pending;if(l!==null){r.shared.pending=null;var a=l,c=a.next;a.next=null,s===null?i=c:s.next=c,s=a;var d=e.alternate;d!==null&&(d=d.updateQueue,l=d.lastBaseUpdate,l!==s&&(l===null?d.firstBaseUpdate=c:l.next=c,d.lastBaseUpdate=a))}if(i!==null){var m=r.baseState;s=0,d=c=a=null,l=i;do{var f=l.lane,k=l.eventTime;if((o&f)===f){d!==null&&(d=d.next={eventTime:k,lane:0,tag:l.tag,payload:l.payload,callback:l.callback,next:null});e:{var w=e,C=l;switch(f=t,k=n,C.tag){case 1:if(w=C.payload,typeof w=="function"){m=w.call(k,m,f);break e}m=w;break e;case 3:w.flags=w.flags&-65537|128;case 0:if(w=C.payload,f=typeof w=="function"?w.call(k,m,f):w,f==null)break e;m=It({},m,f);break e;case 2:ir=!0}}l.callback!==null&&l.lane!==0&&(e.flags|=64,f=r.effects,f===null?r.effects=[l]:f.push(l))}else k={eventTime:k,lane:f,tag:l.tag,payload:l.payload,callback:l.callback,next:null},d===null?(c=d=k,a=m):d=d.next=k,s|=f;if(l=l.next,l===null){if(l=r.shared.pending,l===null)break;f=l,l=f.next,f.next=null,r.lastBaseUpdate=f,r.shared.pending=null}}while(!0);if(d===null&&(a=m),r.baseState=a,r.firstBaseUpdate=c,r.lastBaseUpdate=d,t=r.shared.interleaved,t!==null){r=t;do s|=r.lane,r=r.next;while(r!==t)}else i===null&&(r.shared.lanes=0);ti|=s,e.lanes=s,e.memoizedState=m}}function ug(e,t,n){if(e=t.effects,t.effects=null,e!==null)for(t=0;t<e.length;t++){var o=e[t],r=o.callback;if(r!==null){if(o.callback=null,o=n,typeof r!="function")throw Error(Q(191,r));r.call(o)}}}var Ml={},Mo=wr(Ml),yl=wr(Ml),xl=wr(Ml);function Gr(e){if(e===Ml)throw Error(Q(174));return e}function G_(e,t){switch(St(xl,t),St(yl,e),St(Mo,Ml),e=t.nodeType,e){case 9:case 11:t=(t=t.documentElement)?t.namespaceURI:Wd(null,"");break;default:e=e===8?t.parentNode:t,t=e.namespaceURI||null,e=e.tagName,t=Wd(t,e)}Tt(Mo),St(Mo,t)}function Qi(){Tt(Mo),Tt(yl),Tt(xl)}function $h(e){Gr(xl.current);var t=Gr(Mo.current),n=Wd(t,e.type);t!==n&&(St(yl,e),St(Mo,n))}function q_(e){yl.current===e&&(Tt(Mo),Tt(yl))}var Pt=wr(0);function bc(e){for(var t=e;t!==null;){if(t.tag===13){var n=t.memoizedState;if(n!==null&&(n=n.dehydrated,n===null||n.data==="$?"||n.data==="$!"))return t}else if(t.tag===19&&t.memoizedProps.revealOrder!==void 0){if((t.flags&128)!==0)return t}else if(t.child!==null){t.child.return=t,t=t.child;continue}if(t===e)break;for(;t.sibling===null;){if(t.return===null||t.return===e)return null;t=t.return}t.sibling.return=t.return,t=t.sibling}return null}var Cd=[];function K_(){for(var e=0;e<Cd.length;e++)Cd[e]._workInProgressVersionPrimary=null;Cd.length=0}var ec=Qo.ReactCurrentDispatcher,Ed=Qo.ReactCurrentBatchConfig,ei=0,Nt=null,jt=null,Qt=null,kc=!1,ol=!1,vl=0,Lx=0;function dn(){throw Error(Q(321))}function J_(e,t){if(t===null)return!1;for(var n=0;n<t.length&&n<e.length;n++)if(!_o(e[n],t[n]))return!1;return!0}function Z_(e,t,n,o,r,i){if(ei=i,Nt=t,t.memoizedState=null,t.updateQueue=null,t.lanes=0,ec.current=e===null||e.memoizedState===null?Ix:Dx,e=n(o,r),ol){i=0;do{if(ol=!1,vl=0,25<=i)throw Error(Q(301));i+=1,Qt=jt=null,t.updateQueue=null,ec.current=Ax,e=n(o,r)}while(ol)}if(ec.current=Sc,t=jt!==null&&jt.next!==null,ei=0,Qt=jt=Nt=null,kc=!1,t)throw Error(Q(300));return e}function em(){var e=vl!==0;return vl=0,e}function So(){var e={memoizedState:null,baseState:null,baseQueue:null,queue:null,next:null};return Qt===null?Nt.memoizedState=Qt=e:Qt=Qt.next=e,Qt}function Kn(){if(jt===null){var e=Nt.alternate;e=e!==null?e.memoizedState:null}else e=jt.next;var t=Qt===null?Nt.memoizedState:Qt.next;if(t!==null)Qt=t,jt=e;else{if(e===null)throw Error(Q(310));jt=e,e={memoizedState:jt.memoizedState,baseState:jt.baseState,baseQueue:jt.baseQueue,queue:jt.queue,next:null},Qt===null?Nt.memoizedState=Qt=e:Qt=Qt.next=e}return Qt}function wl(e,t){return typeof t=="function"?t(e):t}function Md(e){var t=Kn(),n=t.queue;if(n===null)throw Error(Q(311));n.lastRenderedReducer=e;var o=jt,r=o.baseQueue,i=n.pending;if(i!==null){if(r!==null){var s=r.next;r.next=i.next,i.next=s}o.baseQueue=r=i,n.pending=null}if(r!==null){i=r.next,o=o.baseState;var l=s=null,a=null,c=i;do{var d=c.lane;if((ei&d)===d)a!==null&&(a=a.next={lane:0,action:c.action,hasEagerState:c.hasEagerState,eagerState:c.eagerState,next:null}),o=c.hasEagerState?c.eagerState:e(o,c.action);else{var m={lane:d,action:c.action,hasEagerState:c.hasEagerState,eagerState:c.eagerState,next:null};a===null?(l=a=m,s=o):a=a.next=m,Nt.lanes|=d,ti|=d}c=c.next}while(c!==null&&c!==i);a===null?s=o:a.next=l,_o(o,t.memoizedState)||(Cn=!0),t.memoizedState=o,t.baseState=s,t.baseQueue=a,n.lastRenderedState=o}if(e=n.interleaved,e!==null){r=e;do i=r.lane,Nt.lanes|=i,ti|=i,r=r.next;while(r!==e)}else r===null&&(n.lanes=0);return[t.memoizedState,n.dispatch]}function Td(e){var t=Kn(),n=t.queue;if(n===null)throw Error(Q(311));n.lastRenderedReducer=e;var o=n.dispatch,r=n.pending,i=t.memoizedState;if(r!==null){n.pending=null;var s=r=r.next;do i=e(i,s.action),s=s.next;while(s!==r);_o(i,t.memoizedState)||(Cn=!0),t.memoizedState=i,t.baseQueue===null&&(t.baseState=i),n.lastRenderedState=i}return[i,o]}function Fh(){}function Oh(e,t){var n=Nt,o=Kn(),r=t(),i=!_o(o.memoizedState,r);if(i&&(o.memoizedState=r,Cn=!0),o=o.queue,tm(Hh.bind(null,n,o,e),[e]),o.getSnapshot!==t||i||Qt!==null&&Qt.memoizedState.tag&1){if(n.flags|=2048,bl(9,zh.bind(null,n,o,r,t),void 0,null),Gt===null)throw Error(Q(349));(ei&30)!==0||Bh(n,t,r)}return r}function Bh(e,t,n){e.flags|=16384,e={getSnapshot:t,value:n},t=Nt.updateQueue,t===null?(t={lastEffect:null,stores:null},Nt.updateQueue=t,t.stores=[e]):(n=t.stores,n===null?t.stores=[e]:n.push(e))}function zh(e,t,n,o){t.value=n,t.getSnapshot=o,Wh(t)&&Uh(e)}function Hh(e,t,n){return n(function(){Wh(t)&&Uh(e)})}function Wh(e){var t=e.getSnapshot;e=e.value;try{var n=t();return!_o(e,n)}catch{return!0}}function Uh(e){var t=Vo(e,1);t!==null&&uo(t,e,1,-1)}function dg(e){var t=So();return typeof e=="function"&&(e=e()),t.memoizedState=t.baseState=e,e={pending:null,interleaved:null,lanes:0,dispatch:null,lastRenderedReducer:wl,lastRenderedState:e},t.queue=e,e=e.dispatch=Nx.bind(null,Nt,e),[t.memoizedState,e]}function bl(e,t,n,o){return e={tag:e,create:t,destroy:n,deps:o,next:null},t=Nt.updateQueue,t===null?(t={lastEffect:null,stores:null},Nt.updateQueue=t,t.lastEffect=e.next=e):(n=t.lastEffect,n===null?t.lastEffect=e.next=e:(o=n.next,n.next=e,e.next=o,t.lastEffect=e)),e}function jh(){return Kn().memoizedState}function tc(e,t,n,o){var r=So();Nt.flags|=e,r.memoizedState=bl(1|t,n,void 0,o===void 0?null:o)}function $c(e,t,n,o){var r=Kn();o=o===void 0?null:o;var i=void 0;if(jt!==null){var s=jt.memoizedState;if(i=s.destroy,o!==null&&J_(o,s.deps)){r.memoizedState=bl(t,n,i,o);return}}Nt.flags|=e,r.memoizedState=bl(1|t,n,i,o)}function _g(e,t){return tc(8390656,8,e,t)}function tm(e,t){return $c(2048,8,e,t)}function Yh(e,t){return $c(4,2,e,t)}function Vh(e,t){return $c(4,4,e,t)}function Xh(e,t){if(typeof t=="function")return e=e(),t(e),function(){t(null)};if(t!=null)return e=e(),t.current=e,function(){t.current=null}}function Qh(e,t,n){return n=n!=null?n.concat([e]):null,$c(4,4,Xh.bind(null,t,e),n)}function nm(){}function Gh(e,t){var n=Kn();t=t===void 0?null:t;var o=n.memoizedState;return o!==null&&t!==null&&J_(t,o[1])?o[0]:(n.memoizedState=[e,t],e)}function qh(e,t){var n=Kn();t=t===void 0?null:t;var o=n.memoizedState;return o!==null&&t!==null&&J_(t,o[1])?o[0]:(e=e(),n.memoizedState=[e,t],e)}function Kh(e,t,n){return(ei&21)===0?(e.baseState&&(e.baseState=!1,Cn=!0),e.memoizedState=n):(_o(n,t)||(n=nh(),Nt.lanes|=n,ti|=n,e.baseState=!0),t)}function Rx(e,t){var n=_t;_t=n!==0&&4>n?n:4,e(!0);var o=Ed.transition;Ed.transition={};try{e(!1),t()}finally{_t=n,Ed.transition=o}}function Jh(){return Kn().memoizedState}function Px(e,t,n){var o=gr(e);if(n={lane:o,action:n,hasEagerState:!1,eagerState:null,next:null},Zh(e))e0(t,n);else if(n=Dh(e,t,n,o),n!==null){var r=xn();uo(n,e,o,r),t0(n,t,o)}}function Nx(e,t,n){var o=gr(e),r={lane:o,action:n,hasEagerState:!1,eagerState:null,next:null};if(Zh(e))e0(t,r);else{var i=e.alternate;if(e.lanes===0&&(i===null||i.lanes===0)&&(i=t.lastRenderedReducer,i!==null))try{var s=t.lastRenderedState,l=i(s,n);if(r.hasEagerState=!0,r.eagerState=l,_o(l,s)){var a=t.interleaved;a===null?(r.next=r,X_(t)):(r.next=a.next,a.next=r),t.interleaved=r;return}}catch{}n=Dh(e,t,r,o),n!==null&&(r=xn(),uo(n,e,o,r),t0(n,t,o))}}function Zh(e){var t=e.alternate;return e===Nt||t!==null&&t===Nt}function e0(e,t){ol=kc=!0;var n=e.pending;n===null?t.next=t:(t.next=n.next,n.next=t),e.pending=t}function t0(e,t,n){if((n&4194240)!==0){var o=t.lanes;o&=e.pendingLanes,n|=o,t.lanes=n,I_(e,n)}}var Sc={readContext:qn,useCallback:dn,useContext:dn,useEffect:dn,useImperativeHandle:dn,useInsertionEffect:dn,useLayoutEffect:dn,useMemo:dn,useReducer:dn,useRef:dn,useState:dn,useDebugValue:dn,useDeferredValue:dn,useTransition:dn,useMutableSource:dn,useSyncExternalStore:dn,useId:dn,unstable_isNewReconciler:!1},Ix={readContext:qn,useCallback:function(e,t){return So().memoizedState=[e,t===void 0?null:t],e},useContext:qn,useEffect:_g,useImperativeHandle:function(e,t,n){return n=n!=null?n.concat([e]):null,tc(4194308,4,Xh.bind(null,t,e),n)},useLayoutEffect:function(e,t){return tc(4194308,4,e,t)},useInsertionEffect:function(e,t){return tc(4,2,e,t)},useMemo:function(e,t){var n=So();return t=t===void 0?null:t,e=e(),n.memoizedState=[e,t],e},useReducer:function(e,t,n){var o=So();return t=n!==void 0?n(t):t,o.memoizedState=o.baseState=t,e={pending:null,interleaved:null,lanes:0,dispatch:null,lastRenderedReducer:e,lastRenderedState:t},o.queue=e,e=e.dispatch=Px.bind(null,Nt,e),[o.memoizedState,e]},useRef:function(e){var t=So();return e={current:e},t.memoizedState=e},useState:dg,useDebugValue:nm,useDeferredValue:function(e){return So().memoizedState=e},useTransition:function(){var e=dg(!1),t=e[0];return e=Rx.bind(null,e[1]),So().memoizedState=e,[t,e]},useMutableSource:function(){},useSyncExternalStore:function(e,t,n){var o=Nt,r=So();if(Rt){if(n===void 0)throw Error(Q(407));n=n()}else{if(n=t(),Gt===null)throw Error(Q(349));(ei&30)!==0||Bh(o,t,n)}r.memoizedState=n;var i={value:n,getSnapshot:t};return r.queue=i,_g(Hh.bind(null,o,i,e),[e]),o.flags|=2048,bl(9,zh.bind(null,o,i,n,t),void 0,null),n},useId:function(){var e=So(),t=Gt.identifierPrefix;if(Rt){var n=Wo,o=Ho;n=(o&~(1<<32-co(o)-1)).toString(32)+n,t=":"+t+"R"+n,n=vl++,0<n&&(t+="H"+n.toString(32)),t+=":"}else n=Lx++,t=":"+t+"r"+n.toString(32)+":";return e.memoizedState=t},unstable_isNewReconciler:!1},Dx={readContext:qn,useCallback:Gh,useContext:qn,useEffect:tm,useImperativeHandle:Qh,useInsertionEffect:Yh,useLayoutEffect:Vh,useMemo:qh,useReducer:Md,useRef:jh,useState:function(){return Md(wl)},useDebugValue:nm,useDeferredValue:function(e){var t=Kn();return Kh(t,jt.memoizedState,e)},useTransition:function(){var e=Md(wl)[0],t=Kn().memoizedState;return[e,t]},useMutableSource:Fh,useSyncExternalStore:Oh,useId:Jh,unstable_isNewReconciler:!1},Ax={readContext:qn,useCallback:Gh,useContext:qn,useEffect:tm,useImperativeHandle:Qh,useInsertionEffect:Yh,useLayoutEffect:Vh,useMemo:qh,useReducer:Td,useRef:jh,useState:function(){return Td(wl)},useDebugValue:nm,useDeferredValue:function(e){var t=Kn();return jt===null?t.memoizedState=e:Kh(t,jt.memoizedState,e)},useTransition:function(){var e=Td(wl)[0],t=Kn().memoizedState;return[e,t]},useMutableSource:Fh,useSyncExternalStore:Oh,useId:Jh,unstable_isNewReconciler:!1};function so(e,t){if(e&&e.defaultProps){t=It({},t),e=e.defaultProps;for(var n in e)t[n]===void 0&&(t[n]=e[n]);return t}return t}function c_(e,t,n,o){t=e.memoizedState,n=n(o,t),n=n==null?t:It({},t,n),e.memoizedState=n,e.lanes===0&&(e.updateQueue.baseState=n)}var Fc={isMounted:function(e){return(e=e._reactInternals)?ri(e)===e:!1},enqueueSetState:function(e,t,n){e=e._reactInternals;var o=xn(),r=gr(e),i=Uo(o,r);i.payload=t,n!=null&&(i.callback=n),t=pr(e,i,r),t!==null&&(uo(t,e,r,o),Za(t,e,r))},enqueueReplaceState:function(e,t,n){e=e._reactInternals;var o=xn(),r=gr(e),i=Uo(o,r);i.tag=1,i.payload=t,n!=null&&(i.callback=n),t=pr(e,i,r),t!==null&&(uo(t,e,r,o),Za(t,e,r))},enqueueForceUpdate:function(e,t){e=e._reactInternals;var n=xn(),o=gr(e),r=Uo(n,o);r.tag=2,t!=null&&(r.callback=t),t=pr(e,r,o),t!==null&&(uo(t,e,o,n),Za(t,e,o))}};function mg(e,t,n,o,r,i,s){return e=e.stateNode,typeof e.shouldComponentUpdate=="function"?e.shouldComponentUpdate(o,i,s):t.prototype&&t.prototype.isPureReactComponent?!pl(n,o)||!pl(r,i):!0}function n0(e,t,n){var o=!1,r=xr,i=t.contextType;return typeof i=="object"&&i!==null?i=qn(i):(r=Mn(t)?Jr:pn.current,o=t.contextTypes,i=(o=o!=null)?Yi(e,r):xr),t=new t(n,i),e.memoizedState=t.state!==null&&t.state!==void 0?t.state:null,t.updater=Fc,e.stateNode=t,t._reactInternals=e,o&&(e=e.stateNode,e.__reactInternalMemoizedUnmaskedChildContext=r,e.__reactInternalMemoizedMaskedChildContext=i),t}function pg(e,t,n,o){e=t.state,typeof t.componentWillReceiveProps=="function"&&t.componentWillReceiveProps(n,o),typeof t.UNSAFE_componentWillReceiveProps=="function"&&t.UNSAFE_componentWillReceiveProps(n,o),t.state!==e&&Fc.enqueueReplaceState(t,t.state,null)}function u_(e,t,n,o){var r=e.stateNode;r.props=n,r.state=e.memoizedState,r.refs={},Q_(e);var i=t.contextType;typeof i=="object"&&i!==null?r.context=qn(i):(i=Mn(t)?Jr:pn.current,r.context=Yi(e,i)),r.state=e.memoizedState,i=t.getDerivedStateFromProps,typeof i=="function"&&(c_(e,t,i,n),r.state=e.memoizedState),typeof t.getDerivedStateFromProps=="function"||typeof r.getSnapshotBeforeUpdate=="function"||typeof r.UNSAFE_componentWillMount!="function"&&typeof r.componentWillMount!="function"||(t=r.state,typeof r.componentWillMount=="function"&&r.componentWillMount(),typeof r.UNSAFE_componentWillMount=="function"&&r.UNSAFE_componentWillMount(),t!==r.state&&Fc.enqueueReplaceState(r,r.state,null),wc(e,n,r,o),r.state=e.memoizedState),typeof r.componentDidMount=="function"&&(e.flags|=4194308)}function Gi(e,t){try{var n="",o=t;do n+=d2(o),o=o.return;while(o);var r=n}catch(i){r=`
Error generating stack: `+i.message+`
`+i.stack}return{value:e,source:t,stack:r,digest:null}}function Ld(e,t,n){return{value:e,source:null,stack:n??null,digest:t??null}}function d_(e,t){try{console.error(t.value)}catch(n){setTimeout(function(){throw n})}}var $x=typeof WeakMap=="function"?WeakMap:Map;function o0(e,t,n){n=Uo(-1,n),n.tag=3,n.payload={element:null};var o=t.value;return n.callback=function(){Ec||(Ec=!0,w_=o),d_(e,t)},n}function r0(e,t,n){n=Uo(-1,n),n.tag=3;var o=e.type.getDerivedStateFromError;if(typeof o=="function"){var r=t.value;n.payload=function(){return o(r)},n.callback=function(){d_(e,t)}}var i=e.stateNode;return i!==null&&typeof i.componentDidCatch=="function"&&(n.callback=function(){d_(e,t),typeof o!="function"&&(fr===null?fr=new Set([this]):fr.add(this));var s=t.stack;this.componentDidCatch(t.value,{componentStack:s!==null?s:""})}),n}function fg(e,t,n){var o=e.pingCache;if(o===null){o=e.pingCache=new $x;var r=new Set;o.set(t,r)}else r=o.get(t),r===void 0&&(r=new Set,o.set(t,r));r.has(n)||(r.add(n),e=qx.bind(null,e,t,n),t.then(e,e))}function gg(e){do{var t;if((t=e.tag===13)&&(t=e.memoizedState,t=t!==null?t.dehydrated!==null:!0),t)return e;e=e.return}while(e!==null);return null}function hg(e,t,n,o,r){return(e.mode&1)===0?(e===t?e.flags|=65536:(e.flags|=128,n.flags|=131072,n.flags&=-52805,n.tag===1&&(n.alternate===null?n.tag=17:(t=Uo(-1,1),t.tag=2,pr(n,t,1))),n.lanes|=1),e):(e.flags|=65536,e.lanes=r,e)}var Fx=Qo.ReactCurrentOwner,Cn=!1;function yn(e,t,n,o){t.child=e===null?Ih(t,null,n,o):Xi(t,e.child,n,o)}function yg(e,t,n,o,r){n=n.render;var i=t.ref;return Wi(t,r),o=Z_(e,t,n,o,i,r),n=em(),e!==null&&!Cn?(t.updateQueue=e.updateQueue,t.flags&=-2053,e.lanes&=~r,Xo(e,t,r)):(Rt&&n&&H_(t),t.flags|=1,yn(e,t,o,r),t.child)}function xg(e,t,n,o,r){if(e===null){var i=n.type;return typeof i=="function"&&!um(i)&&i.defaultProps===void 0&&n.compare===null&&n.defaultProps===void 0?(t.tag=15,t.type=i,i0(e,t,i,o,r)):(e=ic(n.type,null,o,t,t.mode,r),e.ref=t.ref,e.return=t,t.child=e)}if(i=e.child,(e.lanes&r)===0){var s=i.memoizedProps;if(n=n.compare,n=n!==null?n:pl,n(s,o)&&e.ref===t.ref)return Xo(e,t,r)}return t.flags|=1,e=hr(i,o),e.ref=t.ref,e.return=t,t.child=e}function i0(e,t,n,o,r){if(e!==null){var i=e.memoizedProps;if(pl(i,o)&&e.ref===t.ref)if(Cn=!1,t.pendingProps=o=i,(e.lanes&r)!==0)(e.flags&131072)!==0&&(Cn=!0);else return t.lanes=e.lanes,Xo(e,t,r)}return __(e,t,n,o,r)}function s0(e,t,n){var o=t.pendingProps,r=o.children,i=e!==null?e.memoizedState:null;if(o.mode==="hidden")if((t.mode&1)===0)t.memoizedState={baseLanes:0,cachePool:null,transitions:null},St(Fi,An),An|=n;else{if((n&1073741824)===0)return e=i!==null?i.baseLanes|n:n,t.lanes=t.childLanes=1073741824,t.memoizedState={baseLanes:e,cachePool:null,transitions:null},t.updateQueue=null,St(Fi,An),An|=e,null;t.memoizedState={baseLanes:0,cachePool:null,transitions:null},o=i!==null?i.baseLanes:n,St(Fi,An),An|=o}else i!==null?(o=i.baseLanes|n,t.memoizedState=null):o=n,St(Fi,An),An|=o;return yn(e,t,r,n),t.child}function l0(e,t){var n=t.ref;(e===null&&n!==null||e!==null&&e.ref!==n)&&(t.flags|=512,t.flags|=2097152)}function __(e,t,n,o,r){var i=Mn(n)?Jr:pn.current;return i=Yi(t,i),Wi(t,r),n=Z_(e,t,n,o,i,r),o=em(),e!==null&&!Cn?(t.updateQueue=e.updateQueue,t.flags&=-2053,e.lanes&=~r,Xo(e,t,r)):(Rt&&o&&H_(t),t.flags|=1,yn(e,t,n,r),t.child)}function vg(e,t,n,o,r){if(Mn(n)){var i=!0;gc(t)}else i=!1;if(Wi(t,r),t.stateNode===null)nc(e,t),n0(t,n,o),u_(t,n,o,r),o=!0;else if(e===null){var s=t.stateNode,l=t.memoizedProps;s.props=l;var a=s.context,c=n.contextType;typeof c=="object"&&c!==null?c=qn(c):(c=Mn(n)?Jr:pn.current,c=Yi(t,c));var d=n.getDerivedStateFromProps,m=typeof d=="function"||typeof s.getSnapshotBeforeUpdate=="function";m||typeof s.UNSAFE_componentWillReceiveProps!="function"&&typeof s.componentWillReceiveProps!="function"||(l!==o||a!==c)&&pg(t,s,o,c),ir=!1;var f=t.memoizedState;s.state=f,wc(t,o,s,r),a=t.memoizedState,l!==o||f!==a||En.current||ir?(typeof d=="function"&&(c_(t,n,d,o),a=t.memoizedState),(l=ir||mg(t,n,l,o,f,a,c))?(m||typeof s.UNSAFE_componentWillMount!="function"&&typeof s.componentWillMount!="function"||(typeof s.componentWillMount=="function"&&s.componentWillMount(),typeof s.UNSAFE_componentWillMount=="function"&&s.UNSAFE_componentWillMount()),typeof s.componentDidMount=="function"&&(t.flags|=4194308)):(typeof s.componentDidMount=="function"&&(t.flags|=4194308),t.memoizedProps=o,t.memoizedState=a),s.props=o,s.state=a,s.context=c,o=l):(typeof s.componentDidMount=="function"&&(t.flags|=4194308),o=!1)}else{s=t.stateNode,Ah(e,t),l=t.memoizedProps,c=t.type===t.elementType?l:so(t.type,l),s.props=c,m=t.pendingProps,f=s.context,a=n.contextType,typeof a=="object"&&a!==null?a=qn(a):(a=Mn(n)?Jr:pn.current,a=Yi(t,a));var k=n.getDerivedStateFromProps;(d=typeof k=="function"||typeof s.getSnapshotBeforeUpdate=="function")||typeof s.UNSAFE_componentWillReceiveProps!="function"&&typeof s.componentWillReceiveProps!="function"||(l!==m||f!==a)&&pg(t,s,o,a),ir=!1,f=t.memoizedState,s.state=f,wc(t,o,s,r);var w=t.memoizedState;l!==m||f!==w||En.current||ir?(typeof k=="function"&&(c_(t,n,k,o),w=t.memoizedState),(c=ir||mg(t,n,c,o,f,w,a)||!1)?(d||typeof s.UNSAFE_componentWillUpdate!="function"&&typeof s.componentWillUpdate!="function"||(typeof s.componentWillUpdate=="function"&&s.componentWillUpdate(o,w,a),typeof s.UNSAFE_componentWillUpdate=="function"&&s.UNSAFE_componentWillUpdate(o,w,a)),typeof s.componentDidUpdate=="function"&&(t.flags|=4),typeof s.getSnapshotBeforeUpdate=="function"&&(t.flags|=1024)):(typeof s.componentDidUpdate!="function"||l===e.memoizedProps&&f===e.memoizedState||(t.flags|=4),typeof s.getSnapshotBeforeUpdate!="function"||l===e.memoizedProps&&f===e.memoizedState||(t.flags|=1024),t.memoizedProps=o,t.memoizedState=w),s.props=o,s.state=w,s.context=a,o=c):(typeof s.componentDidUpdate!="function"||l===e.memoizedProps&&f===e.memoizedState||(t.flags|=4),typeof s.getSnapshotBeforeUpdate!="function"||l===e.memoizedProps&&f===e.memoizedState||(t.flags|=1024),o=!1)}return m_(e,t,n,o,i,r)}function m_(e,t,n,o,r,i){l0(e,t);var s=(t.flags&128)!==0;if(!o&&!s)return r&&ig(t,n,!1),Xo(e,t,i);o=t.stateNode,Fx.current=t;var l=s&&typeof n.getDerivedStateFromError!="function"?null:o.render();return t.flags|=1,e!==null&&s?(t.child=Xi(t,e.child,null,i),t.child=Xi(t,null,l,i)):yn(e,t,l,i),t.memoizedState=o.state,r&&ig(t,n,!0),t.child}function a0(e){var t=e.stateNode;t.pendingContext?rg(e,t.pendingContext,t.pendingContext!==t.context):t.context&&rg(e,t.context,!1),G_(e,t.containerInfo)}function wg(e,t,n,o,r){return Vi(),U_(r),t.flags|=256,yn(e,t,n,o),t.child}var p_={dehydrated:null,treeContext:null,retryLane:0};function f_(e){return{baseLanes:e,cachePool:null,transitions:null}}function c0(e,t,n){var o=t.pendingProps,r=Pt.current,i=!1,s=(t.flags&128)!==0,l;if((l=s)||(l=e!==null&&e.memoizedState===null?!1:(r&2)!==0),l?(i=!0,t.flags&=-129):(e===null||e.memoizedState!==null)&&(r|=1),St(Pt,r&1),e===null)return l_(t),e=t.memoizedState,e!==null&&(e=e.dehydrated,e!==null)?((t.mode&1)===0?t.lanes=1:e.data==="$!"?t.lanes=8:t.lanes=1073741824,null):(s=o.children,e=o.fallback,i?(o=t.mode,i=t.child,s={mode:"hidden",children:s},(o&1)===0&&i!==null?(i.childLanes=0,i.pendingProps=s):i=zc(s,o,0,null),e=Kr(e,o,n,null),i.return=t,e.return=t,i.sibling=e,t.child=i,t.child.memoizedState=f_(n),t.memoizedState=p_,e):om(t,s));if(r=e.memoizedState,r!==null&&(l=r.dehydrated,l!==null))return Ox(e,t,s,o,l,r,n);if(i){i=o.fallback,s=t.mode,r=e.child,l=r.sibling;var a={mode:"hidden",children:o.children};return(s&1)===0&&t.child!==r?(o=t.child,o.childLanes=0,o.pendingProps=a,t.deletions=null):(o=hr(r,a),o.subtreeFlags=r.subtreeFlags&14680064),l!==null?i=hr(l,i):(i=Kr(i,s,n,null),i.flags|=2),i.return=t,o.return=t,o.sibling=i,t.child=o,o=i,i=t.child,s=e.child.memoizedState,s=s===null?f_(n):{baseLanes:s.baseLanes|n,cachePool:null,transitions:s.transitions},i.memoizedState=s,i.childLanes=e.childLanes&~n,t.memoizedState=p_,o}return i=e.child,e=i.sibling,o=hr(i,{mode:"visible",children:o.children}),(t.mode&1)===0&&(o.lanes=n),o.return=t,o.sibling=null,e!==null&&(n=t.deletions,n===null?(t.deletions=[e],t.flags|=16):n.push(e)),t.child=o,t.memoizedState=null,o}function om(e,t){return t=zc({mode:"visible",children:t},e.mode,0,null),t.return=e,e.child=t}function Xa(e,t,n,o){return o!==null&&U_(o),Xi(t,e.child,null,n),e=om(t,t.pendingProps.children),e.flags|=2,t.memoizedState=null,e}function Ox(e,t,n,o,r,i,s){if(n)return t.flags&256?(t.flags&=-257,o=Ld(Error(Q(422))),Xa(e,t,s,o)):t.memoizedState!==null?(t.child=e.child,t.flags|=128,null):(i=o.fallback,r=t.mode,o=zc({mode:"visible",children:o.children},r,0,null),i=Kr(i,r,s,null),i.flags|=2,o.return=t,i.return=t,o.sibling=i,t.child=o,(t.mode&1)!==0&&Xi(t,e.child,null,s),t.child.memoizedState=f_(s),t.memoizedState=p_,i);if((t.mode&1)===0)return Xa(e,t,s,null);if(r.data==="$!"){if(o=r.nextSibling&&r.nextSibling.dataset,o)var l=o.dgst;return o=l,i=Error(Q(419)),o=Ld(i,o,void 0),Xa(e,t,s,o)}if(l=(s&e.childLanes)!==0,Cn||l){if(o=Gt,o!==null){switch(s&-s){case 4:r=2;break;case 16:r=8;break;case 64:case 128:case 256:case 512:case 1024:case 2048:case 4096:case 8192:case 16384:case 32768:case 65536:case 131072:case 262144:case 524288:case 1048576:case 2097152:case 4194304:case 8388608:case 16777216:case 33554432:case 67108864:r=32;break;case 536870912:r=268435456;break;default:r=0}r=(r&(o.suspendedLanes|s))!==0?0:r,r!==0&&r!==i.retryLane&&(i.retryLane=r,Vo(e,r),uo(o,e,r,-1))}return cm(),o=Ld(Error(Q(421))),Xa(e,t,s,o)}return r.data==="$?"?(t.flags|=128,t.child=e.child,t=Kx.bind(null,e),r._reactRetry=t,null):(e=i.treeContext,$n=mr(r.nextSibling),Fn=t,Rt=!0,ao=null,e!==null&&(Vn[Xn++]=Ho,Vn[Xn++]=Wo,Vn[Xn++]=Zr,Ho=e.id,Wo=e.overflow,Zr=t),t=om(t,o.children),t.flags|=4096,t)}function bg(e,t,n){e.lanes|=t;var o=e.alternate;o!==null&&(o.lanes|=t),a_(e.return,t,n)}function Rd(e,t,n,o,r){var i=e.memoizedState;i===null?e.memoizedState={isBackwards:t,rendering:null,renderingStartTime:0,last:o,tail:n,tailMode:r}:(i.isBackwards=t,i.rendering=null,i.renderingStartTime=0,i.last=o,i.tail=n,i.tailMode=r)}function u0(e,t,n){var o=t.pendingProps,r=o.revealOrder,i=o.tail;if(yn(e,t,o.children,n),o=Pt.current,(o&2)!==0)o=o&1|2,t.flags|=128;else{if(e!==null&&(e.flags&128)!==0)e:for(e=t.child;e!==null;){if(e.tag===13)e.memoizedState!==null&&bg(e,n,t);else if(e.tag===19)bg(e,n,t);else if(e.child!==null){e.child.return=e,e=e.child;continue}if(e===t)break e;for(;e.sibling===null;){if(e.return===null||e.return===t)break e;e=e.return}e.sibling.return=e.return,e=e.sibling}o&=1}if(St(Pt,o),(t.mode&1)===0)t.memoizedState=null;else switch(r){case"forwards":for(n=t.child,r=null;n!==null;)e=n.alternate,e!==null&&bc(e)===null&&(r=n),n=n.sibling;n=r,n===null?(r=t.child,t.child=null):(r=n.sibling,n.sibling=null),Rd(t,!1,r,n,i);break;case"backwards":for(n=null,r=t.child,t.child=null;r!==null;){if(e=r.alternate,e!==null&&bc(e)===null){t.child=r;break}e=r.sibling,r.sibling=n,n=r,r=e}Rd(t,!0,n,null,i);break;case"together":Rd(t,!1,null,null,void 0);break;default:t.memoizedState=null}return t.child}function nc(e,t){(t.mode&1)===0&&e!==null&&(e.alternate=null,t.alternate=null,t.flags|=2)}function Xo(e,t,n){if(e!==null&&(t.dependencies=e.dependencies),ti|=t.lanes,(n&t.childLanes)===0)return null;if(e!==null&&t.child!==e.child)throw Error(Q(153));if(t.child!==null){for(e=t.child,n=hr(e,e.pendingProps),t.child=n,n.return=t;e.sibling!==null;)e=e.sibling,n=n.sibling=hr(e,e.pendingProps),n.return=t;n.sibling=null}return t.child}function Bx(e,t,n){switch(t.tag){case 3:a0(t),Vi();break;case 5:$h(t);break;case 1:Mn(t.type)&&gc(t);break;case 4:G_(t,t.stateNode.containerInfo);break;case 10:var o=t.type._context,r=t.memoizedProps.value;St(xc,o._currentValue),o._currentValue=r;break;case 13:if(o=t.memoizedState,o!==null)return o.dehydrated!==null?(St(Pt,Pt.current&1),t.flags|=128,null):(n&t.child.childLanes)!==0?c0(e,t,n):(St(Pt,Pt.current&1),e=Xo(e,t,n),e!==null?e.sibling:null);St(Pt,Pt.current&1);break;case 19:if(o=(n&t.childLanes)!==0,(e.flags&128)!==0){if(o)return u0(e,t,n);t.flags|=128}if(r=t.memoizedState,r!==null&&(r.rendering=null,r.tail=null,r.lastEffect=null),St(Pt,Pt.current),o)break;return null;case 22:case 23:return t.lanes=0,s0(e,t,n)}return Xo(e,t,n)}var d0,g_,_0,m0;d0=function(e,t){for(var n=t.child;n!==null;){if(n.tag===5||n.tag===6)e.appendChild(n.stateNode);else if(n.tag!==4&&n.child!==null){n.child.return=n,n=n.child;continue}if(n===t)break;for(;n.sibling===null;){if(n.return===null||n.return===t)return;n=n.return}n.sibling.return=n.return,n=n.sibling}};g_=function(){};_0=function(e,t,n,o){var r=e.memoizedProps;if(r!==o){e=t.stateNode,Gr(Mo.current);var i=null;switch(n){case"input":r=Od(e,r),o=Od(e,o),i=[];break;case"select":r=It({},r,{value:void 0}),o=It({},o,{value:void 0}),i=[];break;case"textarea":r=Hd(e,r),o=Hd(e,o),i=[];break;default:typeof r.onClick!="function"&&typeof o.onClick=="function"&&(e.onclick=pc)}Ud(n,o);var s;n=null;for(c in r)if(!o.hasOwnProperty(c)&&r.hasOwnProperty(c)&&r[c]!=null)if(c==="style"){var l=r[c];for(s in l)l.hasOwnProperty(s)&&(n||(n={}),n[s]="")}else c!=="dangerouslySetInnerHTML"&&c!=="children"&&c!=="suppressContentEditableWarning"&&c!=="suppressHydrationWarning"&&c!=="autoFocus"&&(ll.hasOwnProperty(c)?i||(i=[]):(i=i||[]).push(c,null));for(c in o){var a=o[c];if(l=r?.[c],o.hasOwnProperty(c)&&a!==l&&(a!=null||l!=null))if(c==="style")if(l){for(s in l)!l.hasOwnProperty(s)||a&&a.hasOwnProperty(s)||(n||(n={}),n[s]="");for(s in a)a.hasOwnProperty(s)&&l[s]!==a[s]&&(n||(n={}),n[s]=a[s])}else n||(i||(i=[]),i.push(c,n)),n=a;else c==="dangerouslySetInnerHTML"?(a=a?a.__html:void 0,l=l?l.__html:void 0,a!=null&&l!==a&&(i=i||[]).push(c,a)):c==="children"?typeof a!="string"&&typeof a!="number"||(i=i||[]).push(c,""+a):c!=="suppressContentEditableWarning"&&c!=="suppressHydrationWarning"&&(ll.hasOwnProperty(c)?(a!=null&&c==="onScroll"&&Mt("scroll",e),i||l===a||(i=[])):(i=i||[]).push(c,a))}n&&(i=i||[]).push("style",n);var c=i;(t.updateQueue=c)&&(t.flags|=4)}};m0=function(e,t,n,o){n!==o&&(t.flags|=4)};function Ys(e,t){if(!Rt)switch(e.tailMode){case"hidden":t=e.tail;for(var n=null;t!==null;)t.alternate!==null&&(n=t),t=t.sibling;n===null?e.tail=null:n.sibling=null;break;case"collapsed":n=e.tail;for(var o=null;n!==null;)n.alternate!==null&&(o=n),n=n.sibling;o===null?t||e.tail===null?e.tail=null:e.tail.sibling=null:o.sibling=null}}function _n(e){var t=e.alternate!==null&&e.alternate.child===e.child,n=0,o=0;if(t)for(var r=e.child;r!==null;)n|=r.lanes|r.childLanes,o|=r.subtreeFlags&14680064,o|=r.flags&14680064,r.return=e,r=r.sibling;else for(r=e.child;r!==null;)n|=r.lanes|r.childLanes,o|=r.subtreeFlags,o|=r.flags,r.return=e,r=r.sibling;return e.subtreeFlags|=o,e.childLanes=n,t}function zx(e,t,n){var o=t.pendingProps;switch(W_(t),t.tag){case 2:case 16:case 15:case 0:case 11:case 7:case 8:case 12:case 9:case 14:return _n(t),null;case 1:return Mn(t.type)&&fc(),_n(t),null;case 3:return o=t.stateNode,Qi(),Tt(En),Tt(pn),K_(),o.pendingContext&&(o.context=o.pendingContext,o.pendingContext=null),(e===null||e.child===null)&&(Ya(t)?t.flags|=4:e===null||e.memoizedState.isDehydrated&&(t.flags&256)===0||(t.flags|=1024,ao!==null&&(S_(ao),ao=null))),g_(e,t),_n(t),null;case 5:q_(t);var r=Gr(xl.current);if(n=t.type,e!==null&&t.stateNode!=null)_0(e,t,n,o,r),e.ref!==t.ref&&(t.flags|=512,t.flags|=2097152);else{if(!o){if(t.stateNode===null)throw Error(Q(166));return _n(t),null}if(e=Gr(Mo.current),Ya(t)){o=t.stateNode,n=t.type;var i=t.memoizedProps;switch(o[Co]=t,o[hl]=i,e=(t.mode&1)!==0,n){case"dialog":Mt("cancel",o),Mt("close",o);break;case"iframe":case"object":case"embed":Mt("load",o);break;case"video":case"audio":for(r=0;r<Ks.length;r++)Mt(Ks[r],o);break;case"source":Mt("error",o);break;case"img":case"image":case"link":Mt("error",o),Mt("load",o);break;case"details":Mt("toggle",o);break;case"input":Rf(o,i),Mt("invalid",o);break;case"select":o._wrapperState={wasMultiple:!!i.multiple},Mt("invalid",o);break;case"textarea":Nf(o,i),Mt("invalid",o)}Ud(n,i),r=null;for(var s in i)if(i.hasOwnProperty(s)){var l=i[s];s==="children"?typeof l=="string"?o.textContent!==l&&(i.suppressHydrationWarning!==!0&&ja(o.textContent,l,e),r=["children",l]):typeof l=="number"&&o.textContent!==""+l&&(i.suppressHydrationWarning!==!0&&ja(o.textContent,l,e),r=["children",""+l]):ll.hasOwnProperty(s)&&l!=null&&s==="onScroll"&&Mt("scroll",o)}switch(n){case"input":Na(o),Pf(o,i,!0);break;case"textarea":Na(o),If(o);break;case"select":case"option":break;default:typeof i.onClick=="function"&&(o.onclick=pc)}o=r,t.updateQueue=o,o!==null&&(t.flags|=4)}else{s=r.nodeType===9?r:r.ownerDocument,e==="http://www.w3.org/1999/xhtml"&&(e=Hg(n)),e==="http://www.w3.org/1999/xhtml"?n==="script"?(e=s.createElement("div"),e.innerHTML="<script><\/script>",e=e.removeChild(e.firstChild)):typeof o.is=="string"?e=s.createElement(n,{is:o.is}):(e=s.createElement(n),n==="select"&&(s=e,o.multiple?s.multiple=!0:o.size&&(s.size=o.size))):e=s.createElementNS(e,n),e[Co]=t,e[hl]=o,d0(e,t,!1,!1),t.stateNode=e;e:{switch(s=jd(n,o),n){case"dialog":Mt("cancel",e),Mt("close",e),r=o;break;case"iframe":case"object":case"embed":Mt("load",e),r=o;break;case"video":case"audio":for(r=0;r<Ks.length;r++)Mt(Ks[r],e);r=o;break;case"source":Mt("error",e),r=o;break;case"img":case"image":case"link":Mt("error",e),Mt("load",e),r=o;break;case"details":Mt("toggle",e),r=o;break;case"input":Rf(e,o),r=Od(e,o),Mt("invalid",e);break;case"option":r=o;break;case"select":e._wrapperState={wasMultiple:!!o.multiple},r=It({},o,{value:void 0}),Mt("invalid",e);break;case"textarea":Nf(e,o),r=Hd(e,o),Mt("invalid",e);break;default:r=o}Ud(n,r),l=r;for(i in l)if(l.hasOwnProperty(i)){var a=l[i];i==="style"?jg(e,a):i==="dangerouslySetInnerHTML"?(a=a?a.__html:void 0,a!=null&&Wg(e,a)):i==="children"?typeof a=="string"?(n!=="textarea"||a!=="")&&al(e,a):typeof a=="number"&&al(e,""+a):i!=="suppressContentEditableWarning"&&i!=="suppressHydrationWarning"&&i!=="autoFocus"&&(ll.hasOwnProperty(i)?a!=null&&i==="onScroll"&&Mt("scroll",e):a!=null&&M_(e,i,a,s))}switch(n){case"input":Na(e),Pf(e,o,!1);break;case"textarea":Na(e),If(e);break;case"option":o.value!=null&&e.setAttribute("value",""+yr(o.value));break;case"select":e.multiple=!!o.multiple,i=o.value,i!=null?Oi(e,!!o.multiple,i,!1):o.defaultValue!=null&&Oi(e,!!o.multiple,o.defaultValue,!0);break;default:typeof r.onClick=="function"&&(e.onclick=pc)}switch(n){case"button":case"input":case"select":case"textarea":o=!!o.autoFocus;break e;case"img":o=!0;break e;default:o=!1}}o&&(t.flags|=4)}t.ref!==null&&(t.flags|=512,t.flags|=2097152)}return _n(t),null;case 6:if(e&&t.stateNode!=null)m0(e,t,e.memoizedProps,o);else{if(typeof o!="string"&&t.stateNode===null)throw Error(Q(166));if(n=Gr(xl.current),Gr(Mo.current),Ya(t)){if(o=t.stateNode,n=t.memoizedProps,o[Co]=t,(i=o.nodeValue!==n)&&(e=Fn,e!==null))switch(e.tag){case 3:ja(o.nodeValue,n,(e.mode&1)!==0);break;case 5:e.memoizedProps.suppressHydrationWarning!==!0&&ja(o.nodeValue,n,(e.mode&1)!==0)}i&&(t.flags|=4)}else o=(n.nodeType===9?n:n.ownerDocument).createTextNode(o),o[Co]=t,t.stateNode=o}return _n(t),null;case 13:if(Tt(Pt),o=t.memoizedState,e===null||e.memoizedState!==null&&e.memoizedState.dehydrated!==null){if(Rt&&$n!==null&&(t.mode&1)!==0&&(t.flags&128)===0)Ph(),Vi(),t.flags|=98560,i=!1;else if(i=Ya(t),o!==null&&o.dehydrated!==null){if(e===null){if(!i)throw Error(Q(318));if(i=t.memoizedState,i=i!==null?i.dehydrated:null,!i)throw Error(Q(317));i[Co]=t}else Vi(),(t.flags&128)===0&&(t.memoizedState=null),t.flags|=4;_n(t),i=!1}else ao!==null&&(S_(ao),ao=null),i=!0;if(!i)return t.flags&65536?t:null}return(t.flags&128)!==0?(t.lanes=n,t):(o=o!==null,o!==(e!==null&&e.memoizedState!==null)&&o&&(t.child.flags|=8192,(t.mode&1)!==0&&(e===null||(Pt.current&1)!==0?Yt===0&&(Yt=3):cm())),t.updateQueue!==null&&(t.flags|=4),_n(t),null);case 4:return Qi(),g_(e,t),e===null&&fl(t.stateNode.containerInfo),_n(t),null;case 10:return V_(t.type._context),_n(t),null;case 17:return Mn(t.type)&&fc(),_n(t),null;case 19:if(Tt(Pt),i=t.memoizedState,i===null)return _n(t),null;if(o=(t.flags&128)!==0,s=i.rendering,s===null)if(o)Ys(i,!1);else{if(Yt!==0||e!==null&&(e.flags&128)!==0)for(e=t.child;e!==null;){if(s=bc(e),s!==null){for(t.flags|=128,Ys(i,!1),o=s.updateQueue,o!==null&&(t.updateQueue=o,t.flags|=4),t.subtreeFlags=0,o=n,n=t.child;n!==null;)i=n,e=o,i.flags&=14680066,s=i.alternate,s===null?(i.childLanes=0,i.lanes=e,i.child=null,i.subtreeFlags=0,i.memoizedProps=null,i.memoizedState=null,i.updateQueue=null,i.dependencies=null,i.stateNode=null):(i.childLanes=s.childLanes,i.lanes=s.lanes,i.child=s.child,i.subtreeFlags=0,i.deletions=null,i.memoizedProps=s.memoizedProps,i.memoizedState=s.memoizedState,i.updateQueue=s.updateQueue,i.type=s.type,e=s.dependencies,i.dependencies=e===null?null:{lanes:e.lanes,firstContext:e.firstContext}),n=n.sibling;return St(Pt,Pt.current&1|2),t.child}e=e.sibling}i.tail!==null&&Ot()>qi&&(t.flags|=128,o=!0,Ys(i,!1),t.lanes=4194304)}else{if(!o)if(e=bc(s),e!==null){if(t.flags|=128,o=!0,n=e.updateQueue,n!==null&&(t.updateQueue=n,t.flags|=4),Ys(i,!0),i.tail===null&&i.tailMode==="hidden"&&!s.alternate&&!Rt)return _n(t),null}else 2*Ot()-i.renderingStartTime>qi&&n!==1073741824&&(t.flags|=128,o=!0,Ys(i,!1),t.lanes=4194304);i.isBackwards?(s.sibling=t.child,t.child=s):(n=i.last,n!==null?n.sibling=s:t.child=s,i.last=s)}return i.tail!==null?(t=i.tail,i.rendering=t,i.tail=t.sibling,i.renderingStartTime=Ot(),t.sibling=null,n=Pt.current,St(Pt,o?n&1|2:n&1),t):(_n(t),null);case 22:case 23:return am(),o=t.memoizedState!==null,e!==null&&e.memoizedState!==null!==o&&(t.flags|=8192),o&&(t.mode&1)!==0?(An&1073741824)!==0&&(_n(t),t.subtreeFlags&6&&(t.flags|=8192)):_n(t),null;case 24:return null;case 25:return null}throw Error(Q(156,t.tag))}function Hx(e,t){switch(W_(t),t.tag){case 1:return Mn(t.type)&&fc(),e=t.flags,e&65536?(t.flags=e&-65537|128,t):null;case 3:return Qi(),Tt(En),Tt(pn),K_(),e=t.flags,(e&65536)!==0&&(e&128)===0?(t.flags=e&-65537|128,t):null;case 5:return q_(t),null;case 13:if(Tt(Pt),e=t.memoizedState,e!==null&&e.dehydrated!==null){if(t.alternate===null)throw Error(Q(340));Vi()}return e=t.flags,e&65536?(t.flags=e&-65537|128,t):null;case 19:return Tt(Pt),null;case 4:return Qi(),null;case 10:return V_(t.type._context),null;case 22:case 23:return am(),null;case 24:return null;default:return null}}var Qa=!1,mn=!1,Wx=typeof WeakSet=="function"?WeakSet:Set,oe=null;function $i(e,t){var n=e.ref;if(n!==null)if(typeof n=="function")try{n(null)}catch(o){$t(e,t,o)}else n.current=null}function h_(e,t,n){try{n()}catch(o){$t(e,t,o)}}var kg=!1;function Ux(e,t){if(e_=dc,e=yh(),z_(e)){if("selectionStart"in e)var n={start:e.selectionStart,end:e.selectionEnd};else e:{n=(n=e.ownerDocument)&&n.defaultView||window;var o=n.getSelection&&n.getSelection();if(o&&o.rangeCount!==0){n=o.anchorNode;var r=o.anchorOffset,i=o.focusNode;o=o.focusOffset;try{n.nodeType,i.nodeType}catch{n=null;break e}var s=0,l=-1,a=-1,c=0,d=0,m=e,f=null;t:for(;;){for(var k;m!==n||r!==0&&m.nodeType!==3||(l=s+r),m!==i||o!==0&&m.nodeType!==3||(a=s+o),m.nodeType===3&&(s+=m.nodeValue.length),(k=m.firstChild)!==null;)f=m,m=k;for(;;){if(m===e)break t;if(f===n&&++c===r&&(l=s),f===i&&++d===o&&(a=s),(k=m.nextSibling)!==null)break;m=f,f=m.parentNode}m=k}n=l===-1||a===-1?null:{start:l,end:a}}else n=null}n=n||{start:0,end:0}}else n=null;for(t_={focusedElem:e,selectionRange:n},dc=!1,oe=t;oe!==null;)if(t=oe,e=t.child,(t.subtreeFlags&1028)!==0&&e!==null)e.return=t,oe=e;else for(;oe!==null;){t=oe;try{var w=t.alternate;if((t.flags&1024)!==0)switch(t.tag){case 0:case 11:case 15:break;case 1:if(w!==null){var C=w.memoizedProps,S=w.memoizedState,g=t.stateNode,x=g.getSnapshotBeforeUpdate(t.elementType===t.type?C:so(t.type,C),S);g.__reactInternalSnapshotBeforeUpdate=x}break;case 3:var h=t.stateNode.containerInfo;h.nodeType===1?h.textContent="":h.nodeType===9&&h.documentElement&&h.removeChild(h.documentElement);break;case 5:case 6:case 4:case 17:break;default:throw Error(Q(163))}}catch(E){$t(t,t.return,E)}if(e=t.sibling,e!==null){e.return=t.return,oe=e;break}oe=t.return}return w=kg,kg=!1,w}function rl(e,t,n){var o=t.updateQueue;if(o=o!==null?o.lastEffect:null,o!==null){var r=o=o.next;do{if((r.tag&e)===e){var i=r.destroy;r.destroy=void 0,i!==void 0&&h_(t,n,i)}r=r.next}while(r!==o)}}function Oc(e,t){if(t=t.updateQueue,t=t!==null?t.lastEffect:null,t!==null){var n=t=t.next;do{if((n.tag&e)===e){var o=n.create;n.destroy=o()}n=n.next}while(n!==t)}}function y_(e){var t=e.ref;if(t!==null){var n=e.stateNode;e.tag,e=n,typeof t=="function"?t(e):t.current=e}}function p0(e){var t=e.alternate;t!==null&&(e.alternate=null,p0(t)),e.child=null,e.deletions=null,e.sibling=null,e.tag===5&&(t=e.stateNode,t!==null&&(delete t[Co],delete t[hl],delete t[r_],delete t[Cx],delete t[Ex])),e.stateNode=null,e.return=null,e.dependencies=null,e.memoizedProps=null,e.memoizedState=null,e.pendingProps=null,e.stateNode=null,e.updateQueue=null}function f0(e){return e.tag===5||e.tag===3||e.tag===4}function Sg(e){e:for(;;){for(;e.sibling===null;){if(e.return===null||f0(e.return))return null;e=e.return}for(e.sibling.return=e.return,e=e.sibling;e.tag!==5&&e.tag!==6&&e.tag!==18;){if(e.flags&2||e.child===null||e.tag===4)continue e;e.child.return=e,e=e.child}if(!(e.flags&2))return e.stateNode}}function x_(e,t,n){var o=e.tag;if(o===5||o===6)e=e.stateNode,t?n.nodeType===8?n.parentNode.insertBefore(e,t):n.insertBefore(e,t):(n.nodeType===8?(t=n.parentNode,t.insertBefore(e,n)):(t=n,t.appendChild(e)),n=n._reactRootContainer,n!=null||t.onclick!==null||(t.onclick=pc));else if(o!==4&&(e=e.child,e!==null))for(x_(e,t,n),e=e.sibling;e!==null;)x_(e,t,n),e=e.sibling}function v_(e,t,n){var o=e.tag;if(o===5||o===6)e=e.stateNode,t?n.insertBefore(e,t):n.appendChild(e);else if(o!==4&&(e=e.child,e!==null))for(v_(e,t,n),e=e.sibling;e!==null;)v_(e,t,n),e=e.sibling}var nn=null,lo=!1;function or(e,t,n){for(n=n.child;n!==null;)g0(e,t,n),n=n.sibling}function g0(e,t,n){if(Eo&&typeof Eo.onCommitFiberUnmount=="function")try{Eo.onCommitFiberUnmount(Rc,n)}catch{}switch(n.tag){case 5:mn||$i(n,t);case 6:var o=nn,r=lo;nn=null,or(e,t,n),nn=o,lo=r,nn!==null&&(lo?(e=nn,n=n.stateNode,e.nodeType===8?e.parentNode.removeChild(n):e.removeChild(n)):nn.removeChild(n.stateNode));break;case 18:nn!==null&&(lo?(e=nn,n=n.stateNode,e.nodeType===8?kd(e.parentNode,n):e.nodeType===1&&kd(e,n),_l(e)):kd(nn,n.stateNode));break;case 4:o=nn,r=lo,nn=n.stateNode.containerInfo,lo=!0,or(e,t,n),nn=o,lo=r;break;case 0:case 11:case 14:case 15:if(!mn&&(o=n.updateQueue,o!==null&&(o=o.lastEffect,o!==null))){r=o=o.next;do{var i=r,s=i.destroy;i=i.tag,s!==void 0&&((i&2)!==0||(i&4)!==0)&&h_(n,t,s),r=r.next}while(r!==o)}or(e,t,n);break;case 1:if(!mn&&($i(n,t),o=n.stateNode,typeof o.componentWillUnmount=="function"))try{o.props=n.memoizedProps,o.state=n.memoizedState,o.componentWillUnmount()}catch(l){$t(n,t,l)}or(e,t,n);break;case 21:or(e,t,n);break;case 22:n.mode&1?(mn=(o=mn)||n.memoizedState!==null,or(e,t,n),mn=o):or(e,t,n);break;default:or(e,t,n)}}function Cg(e){var t=e.updateQueue;if(t!==null){e.updateQueue=null;var n=e.stateNode;n===null&&(n=e.stateNode=new Wx),t.forEach(function(o){var r=Jx.bind(null,e,o);n.has(o)||(n.add(o),o.then(r,r))})}}function io(e,t){var n=t.deletions;if(n!==null)for(var o=0;o<n.length;o++){var r=n[o];try{var i=e,s=t,l=s;e:for(;l!==null;){switch(l.tag){case 5:nn=l.stateNode,lo=!1;break e;case 3:nn=l.stateNode.containerInfo,lo=!0;break e;case 4:nn=l.stateNode.containerInfo,lo=!0;break e}l=l.return}if(nn===null)throw Error(Q(160));g0(i,s,r),nn=null,lo=!1;var a=r.alternate;a!==null&&(a.return=null),r.return=null}catch(c){$t(r,t,c)}}if(t.subtreeFlags&12854)for(t=t.child;t!==null;)h0(t,e),t=t.sibling}function h0(e,t){var n=e.alternate,o=e.flags;switch(e.tag){case 0:case 11:case 14:case 15:if(io(t,e),ko(e),o&4){try{rl(3,e,e.return),Oc(3,e)}catch(C){$t(e,e.return,C)}try{rl(5,e,e.return)}catch(C){$t(e,e.return,C)}}break;case 1:io(t,e),ko(e),o&512&&n!==null&&$i(n,n.return);break;case 5:if(io(t,e),ko(e),o&512&&n!==null&&$i(n,n.return),e.flags&32){var r=e.stateNode;try{al(r,"")}catch(C){$t(e,e.return,C)}}if(o&4&&(r=e.stateNode,r!=null)){var i=e.memoizedProps,s=n!==null?n.memoizedProps:i,l=e.type,a=e.updateQueue;if(e.updateQueue=null,a!==null)try{l==="input"&&i.type==="radio"&&i.name!=null&&Bg(r,i),jd(l,s);var c=jd(l,i);for(s=0;s<a.length;s+=2){var d=a[s],m=a[s+1];d==="style"?jg(r,m):d==="dangerouslySetInnerHTML"?Wg(r,m):d==="children"?al(r,m):M_(r,d,m,c)}switch(l){case"input":Bd(r,i);break;case"textarea":zg(r,i);break;case"select":var f=r._wrapperState.wasMultiple;r._wrapperState.wasMultiple=!!i.multiple;var k=i.value;k!=null?Oi(r,!!i.multiple,k,!1):f!==!!i.multiple&&(i.defaultValue!=null?Oi(r,!!i.multiple,i.defaultValue,!0):Oi(r,!!i.multiple,i.multiple?[]:"",!1))}r[hl]=i}catch(C){$t(e,e.return,C)}}break;case 6:if(io(t,e),ko(e),o&4){if(e.stateNode===null)throw Error(Q(162));r=e.stateNode,i=e.memoizedProps;try{r.nodeValue=i}catch(C){$t(e,e.return,C)}}break;case 3:if(io(t,e),ko(e),o&4&&n!==null&&n.memoizedState.isDehydrated)try{_l(t.containerInfo)}catch(C){$t(e,e.return,C)}break;case 4:io(t,e),ko(e);break;case 13:io(t,e),ko(e),r=e.child,r.flags&8192&&(i=r.memoizedState!==null,r.stateNode.isHidden=i,!i||r.alternate!==null&&r.alternate.memoizedState!==null||(sm=Ot())),o&4&&Cg(e);break;case 22:if(d=n!==null&&n.memoizedState!==null,e.mode&1?(mn=(c=mn)||d,io(t,e),mn=c):io(t,e),ko(e),o&8192){if(c=e.memoizedState!==null,(e.stateNode.isHidden=c)&&!d&&(e.mode&1)!==0)for(oe=e,d=e.child;d!==null;){for(m=oe=d;oe!==null;){switch(f=oe,k=f.child,f.tag){case 0:case 11:case 14:case 15:rl(4,f,f.return);break;case 1:$i(f,f.return);var w=f.stateNode;if(typeof w.componentWillUnmount=="function"){o=f,n=f.return;try{t=o,w.props=t.memoizedProps,w.state=t.memoizedState,w.componentWillUnmount()}catch(C){$t(o,n,C)}}break;case 5:$i(f,f.return);break;case 22:if(f.memoizedState!==null){Mg(m);continue}}k!==null?(k.return=f,oe=k):Mg(m)}d=d.sibling}e:for(d=null,m=e;;){if(m.tag===5){if(d===null){d=m;try{r=m.stateNode,c?(i=r.style,typeof i.setProperty=="function"?i.setProperty("display","none","important"):i.display="none"):(l=m.stateNode,a=m.memoizedProps.style,s=a!=null&&a.hasOwnProperty("display")?a.display:null,l.style.display=Ug("display",s))}catch(C){$t(e,e.return,C)}}}else if(m.tag===6){if(d===null)try{m.stateNode.nodeValue=c?"":m.memoizedProps}catch(C){$t(e,e.return,C)}}else if((m.tag!==22&&m.tag!==23||m.memoizedState===null||m===e)&&m.child!==null){m.child.return=m,m=m.child;continue}if(m===e)break e;for(;m.sibling===null;){if(m.return===null||m.return===e)break e;d===m&&(d=null),m=m.return}d===m&&(d=null),m.sibling.return=m.return,m=m.sibling}}break;case 19:io(t,e),ko(e),o&4&&Cg(e);break;case 21:break;default:io(t,e),ko(e)}}function ko(e){var t=e.flags;if(t&2){try{e:{for(var n=e.return;n!==null;){if(f0(n)){var o=n;break e}n=n.return}throw Error(Q(160))}switch(o.tag){case 5:var r=o.stateNode;o.flags&32&&(al(r,""),o.flags&=-33);var i=Sg(e);v_(e,i,r);break;case 3:case 4:var s=o.stateNode.containerInfo,l=Sg(e);x_(e,l,s);break;default:throw Error(Q(161))}}catch(a){$t(e,e.return,a)}e.flags&=-3}t&4096&&(e.flags&=-4097)}function jx(e,t,n){oe=e,y0(e,t,n)}function y0(e,t,n){for(var o=(e.mode&1)!==0;oe!==null;){var r=oe,i=r.child;if(r.tag===22&&o){var s=r.memoizedState!==null||Qa;if(!s){var l=r.alternate,a=l!==null&&l.memoizedState!==null||mn;l=Qa;var c=mn;if(Qa=s,(mn=a)&&!c)for(oe=r;oe!==null;)s=oe,a=s.child,s.tag===22&&s.memoizedState!==null?Tg(r):a!==null?(a.return=s,oe=a):Tg(r);for(;i!==null;)oe=i,y0(i,t,n),i=i.sibling;oe=r,Qa=l,mn=c}Eg(e,t,n)}else(r.subtreeFlags&8772)!==0&&i!==null?(i.return=r,oe=i):Eg(e,t,n)}}function Eg(e){for(;oe!==null;){var t=oe;if((t.flags&8772)!==0){var n=t.alternate;try{if((t.flags&8772)!==0)switch(t.tag){case 0:case 11:case 15:mn||Oc(5,t);break;case 1:var o=t.stateNode;if(t.flags&4&&!mn)if(n===null)o.componentDidMount();else{var r=t.elementType===t.type?n.memoizedProps:so(t.type,n.memoizedProps);o.componentDidUpdate(r,n.memoizedState,o.__reactInternalSnapshotBeforeUpdate)}var i=t.updateQueue;i!==null&&ug(t,i,o);break;case 3:var s=t.updateQueue;if(s!==null){if(n=null,t.child!==null)switch(t.child.tag){case 5:n=t.child.stateNode;break;case 1:n=t.child.stateNode}ug(t,s,n)}break;case 5:var l=t.stateNode;if(n===null&&t.flags&4){n=l;var a=t.memoizedProps;switch(t.type){case"button":case"input":case"select":case"textarea":a.autoFocus&&n.focus();break;case"img":a.src&&(n.src=a.src)}}break;case 6:break;case 4:break;case 12:break;case 13:if(t.memoizedState===null){var c=t.alternate;if(c!==null){var d=c.memoizedState;if(d!==null){var m=d.dehydrated;m!==null&&_l(m)}}}break;case 19:case 17:case 21:case 22:case 23:case 25:break;default:throw Error(Q(163))}mn||t.flags&512&&y_(t)}catch(f){$t(t,t.return,f)}}if(t===e){oe=null;break}if(n=t.sibling,n!==null){n.return=t.return,oe=n;break}oe=t.return}}function Mg(e){for(;oe!==null;){var t=oe;if(t===e){oe=null;break}var n=t.sibling;if(n!==null){n.return=t.return,oe=n;break}oe=t.return}}function Tg(e){for(;oe!==null;){var t=oe;try{switch(t.tag){case 0:case 11:case 15:var n=t.return;try{Oc(4,t)}catch(a){$t(t,n,a)}break;case 1:var o=t.stateNode;if(typeof o.componentDidMount=="function"){var r=t.return;try{o.componentDidMount()}catch(a){$t(t,r,a)}}var i=t.return;try{y_(t)}catch(a){$t(t,i,a)}break;case 5:var s=t.return;try{y_(t)}catch(a){$t(t,s,a)}}}catch(a){$t(t,t.return,a)}if(t===e){oe=null;break}var l=t.sibling;if(l!==null){l.return=t.return,oe=l;break}oe=t.return}}var Yx=Math.ceil,Cc=Qo.ReactCurrentDispatcher,rm=Qo.ReactCurrentOwner,Gn=Qo.ReactCurrentBatchConfig,ot=0,Gt=null,zt=null,on=0,An=0,Fi=wr(0),Yt=0,kl=null,ti=0,Bc=0,im=0,il=null,Sn=null,sm=0,qi=1/0,Bo=null,Ec=!1,w_=null,fr=null,Ga=!1,cr=null,Mc=0,sl=0,b_=null,oc=-1,rc=0;function xn(){return(ot&6)!==0?Ot():oc!==-1?oc:oc=Ot()}function gr(e){return(e.mode&1)===0?1:(ot&2)!==0&&on!==0?on&-on:Tx.transition!==null?(rc===0&&(rc=nh()),rc):(e=_t,e!==0||(e=window.event,e=e===void 0?16:ch(e.type)),e)}function uo(e,t,n,o){if(50<sl)throw sl=0,b_=null,Error(Q(185));Sl(e,n,o),((ot&2)===0||e!==Gt)&&(e===Gt&&((ot&2)===0&&(Bc|=n),Yt===4&&lr(e,on)),Tn(e,o),n===1&&ot===0&&(t.mode&1)===0&&(qi=Ot()+500,Ac&&br()))}function Tn(e,t){var n=e.callbackNode;R2(e,t);var o=uc(e,e===Gt?on:0);if(o===0)n!==null&&$f(n),e.callbackNode=null,e.callbackPriority=0;else if(t=o&-o,e.callbackPriority!==t){if(n!=null&&$f(n),t===1)e.tag===0?Mx(Lg.bind(null,e)):Th(Lg.bind(null,e)),kx(function(){(ot&6)===0&&br()}),n=null;else{switch(oh(o)){case 1:n=N_;break;case 4:n=eh;break;case 16:n=cc;break;case 536870912:n=th;break;default:n=cc}n=E0(n,x0.bind(null,e))}e.callbackPriority=t,e.callbackNode=n}}function x0(e,t){if(oc=-1,rc=0,(ot&6)!==0)throw Error(Q(327));var n=e.callbackNode;if(Ui()&&e.callbackNode!==n)return null;var o=uc(e,e===Gt?on:0);if(o===0)return null;if((o&30)!==0||(o&e.expiredLanes)!==0||t)t=Tc(e,o);else{t=o;var r=ot;ot|=2;var i=w0();(Gt!==e||on!==t)&&(Bo=null,qi=Ot()+500,qr(e,t));do try{Qx();break}catch(l){v0(e,l)}while(!0);Y_(),Cc.current=i,ot=r,zt!==null?t=0:(Gt=null,on=0,t=Yt)}if(t!==0){if(t===2&&(r=Gd(e),r!==0&&(o=r,t=k_(e,r))),t===1)throw n=kl,qr(e,0),lr(e,o),Tn(e,Ot()),n;if(t===6)lr(e,o);else{if(r=e.current.alternate,(o&30)===0&&!Vx(r)&&(t=Tc(e,o),t===2&&(i=Gd(e),i!==0&&(o=i,t=k_(e,i))),t===1))throw n=kl,qr(e,0),lr(e,o),Tn(e,Ot()),n;switch(e.finishedWork=r,e.finishedLanes=o,t){case 0:case 1:throw Error(Q(345));case 2:Vr(e,Sn,Bo);break;case 3:if(lr(e,o),(o&130023424)===o&&(t=sm+500-Ot(),10<t)){if(uc(e,0)!==0)break;if(r=e.suspendedLanes,(r&o)!==o){xn(),e.pingedLanes|=e.suspendedLanes&r;break}e.timeoutHandle=o_(Vr.bind(null,e,Sn,Bo),t);break}Vr(e,Sn,Bo);break;case 4:if(lr(e,o),(o&4194240)===o)break;for(t=e.eventTimes,r=-1;0<o;){var s=31-co(o);i=1<<s,s=t[s],s>r&&(r=s),o&=~i}if(o=r,o=Ot()-o,o=(120>o?120:480>o?480:1080>o?1080:1920>o?1920:3e3>o?3e3:4320>o?4320:1960*Yx(o/1960))-o,10<o){e.timeoutHandle=o_(Vr.bind(null,e,Sn,Bo),o);break}Vr(e,Sn,Bo);break;case 5:Vr(e,Sn,Bo);break;default:throw Error(Q(329))}}}return Tn(e,Ot()),e.callbackNode===n?x0.bind(null,e):null}function k_(e,t){var n=il;return e.current.memoizedState.isDehydrated&&(qr(e,t).flags|=256),e=Tc(e,t),e!==2&&(t=Sn,Sn=n,t!==null&&S_(t)),e}function S_(e){Sn===null?Sn=e:Sn.push.apply(Sn,e)}function Vx(e){for(var t=e;;){if(t.flags&16384){var n=t.updateQueue;if(n!==null&&(n=n.stores,n!==null))for(var o=0;o<n.length;o++){var r=n[o],i=r.getSnapshot;r=r.value;try{if(!_o(i(),r))return!1}catch{return!1}}}if(n=t.child,t.subtreeFlags&16384&&n!==null)n.return=t,t=n;else{if(t===e)break;for(;t.sibling===null;){if(t.return===null||t.return===e)return!0;t=t.return}t.sibling.return=t.return,t=t.sibling}}return!0}function lr(e,t){for(t&=~im,t&=~Bc,e.suspendedLanes|=t,e.pingedLanes&=~t,e=e.expirationTimes;0<t;){var n=31-co(t),o=1<<n;e[n]=-1,t&=~o}}function Lg(e){if((ot&6)!==0)throw Error(Q(327));Ui();var t=uc(e,0);if((t&1)===0)return Tn(e,Ot()),null;var n=Tc(e,t);if(e.tag!==0&&n===2){var o=Gd(e);o!==0&&(t=o,n=k_(e,o))}if(n===1)throw n=kl,qr(e,0),lr(e,t),Tn(e,Ot()),n;if(n===6)throw Error(Q(345));return e.finishedWork=e.current.alternate,e.finishedLanes=t,Vr(e,Sn,Bo),Tn(e,Ot()),null}function lm(e,t){var n=ot;ot|=1;try{return e(t)}finally{ot=n,ot===0&&(qi=Ot()+500,Ac&&br())}}function ni(e){cr!==null&&cr.tag===0&&(ot&6)===0&&Ui();var t=ot;ot|=1;var n=Gn.transition,o=_t;try{if(Gn.transition=null,_t=1,e)return e()}finally{_t=o,Gn.transition=n,ot=t,(ot&6)===0&&br()}}function am(){An=Fi.current,Tt(Fi)}function qr(e,t){e.finishedWork=null,e.finishedLanes=0;var n=e.timeoutHandle;if(n!==-1&&(e.timeoutHandle=-1,bx(n)),zt!==null)for(n=zt.return;n!==null;){var o=n;switch(W_(o),o.tag){case 1:o=o.type.childContextTypes,o!=null&&fc();break;case 3:Qi(),Tt(En),Tt(pn),K_();break;case 5:q_(o);break;case 4:Qi();break;case 13:Tt(Pt);break;case 19:Tt(Pt);break;case 10:V_(o.type._context);break;case 22:case 23:am()}n=n.return}if(Gt=e,zt=e=hr(e.current,null),on=An=t,Yt=0,kl=null,im=Bc=ti=0,Sn=il=null,Qr!==null){for(t=0;t<Qr.length;t++)if(n=Qr[t],o=n.interleaved,o!==null){n.interleaved=null;var r=o.next,i=n.pending;if(i!==null){var s=i.next;i.next=r,o.next=s}n.pending=o}Qr=null}return e}function v0(e,t){do{var n=zt;try{if(Y_(),ec.current=Sc,kc){for(var o=Nt.memoizedState;o!==null;){var r=o.queue;r!==null&&(r.pending=null),o=o.next}kc=!1}if(ei=0,Qt=jt=Nt=null,ol=!1,vl=0,rm.current=null,n===null||n.return===null){Yt=1,kl=t,zt=null;break}e:{var i=e,s=n.return,l=n,a=t;if(t=on,l.flags|=32768,a!==null&&typeof a=="object"&&typeof a.then=="function"){var c=a,d=l,m=d.tag;if((d.mode&1)===0&&(m===0||m===11||m===15)){var f=d.alternate;f?(d.updateQueue=f.updateQueue,d.memoizedState=f.memoizedState,d.lanes=f.lanes):(d.updateQueue=null,d.memoizedState=null)}var k=gg(s);if(k!==null){k.flags&=-257,hg(k,s,l,i,t),k.mode&1&&fg(i,c,t),t=k,a=c;var w=t.updateQueue;if(w===null){var C=new Set;C.add(a),t.updateQueue=C}else w.add(a);break e}else{if((t&1)===0){fg(i,c,t),cm();break e}a=Error(Q(426))}}else if(Rt&&l.mode&1){var S=gg(s);if(S!==null){(S.flags&65536)===0&&(S.flags|=256),hg(S,s,l,i,t),U_(Gi(a,l));break e}}i=a=Gi(a,l),Yt!==4&&(Yt=2),il===null?il=[i]:il.push(i),i=s;do{switch(i.tag){case 3:i.flags|=65536,t&=-t,i.lanes|=t;var g=o0(i,a,t);cg(i,g);break e;case 1:l=a;var x=i.type,h=i.stateNode;if((i.flags&128)===0&&(typeof x.getDerivedStateFromError=="function"||h!==null&&typeof h.componentDidCatch=="function"&&(fr===null||!fr.has(h)))){i.flags|=65536,t&=-t,i.lanes|=t;var E=r0(i,l,t);cg(i,E);break e}}i=i.return}while(i!==null)}k0(n)}catch(O){t=O,zt===n&&n!==null&&(zt=n=n.return);continue}break}while(!0)}function w0(){var e=Cc.current;return Cc.current=Sc,e===null?Sc:e}function cm(){(Yt===0||Yt===3||Yt===2)&&(Yt=4),Gt===null||(ti&268435455)===0&&(Bc&268435455)===0||lr(Gt,on)}function Tc(e,t){var n=ot;ot|=2;var o=w0();(Gt!==e||on!==t)&&(Bo=null,qr(e,t));do try{Xx();break}catch(r){v0(e,r)}while(!0);if(Y_(),ot=n,Cc.current=o,zt!==null)throw Error(Q(261));return Gt=null,on=0,Yt}function Xx(){for(;zt!==null;)b0(zt)}function Qx(){for(;zt!==null&&!w2();)b0(zt)}function b0(e){var t=C0(e.alternate,e,An);e.memoizedProps=e.pendingProps,t===null?k0(e):zt=t,rm.current=null}function k0(e){var t=e;do{var n=t.alternate;if(e=t.return,(t.flags&32768)===0){if(n=zx(n,t,An),n!==null){zt=n;return}}else{if(n=Hx(n,t),n!==null){n.flags&=32767,zt=n;return}if(e!==null)e.flags|=32768,e.subtreeFlags=0,e.deletions=null;else{Yt=6,zt=null;return}}if(t=t.sibling,t!==null){zt=t;return}zt=t=e}while(t!==null);Yt===0&&(Yt=5)}function Vr(e,t,n){var o=_t,r=Gn.transition;try{Gn.transition=null,_t=1,Gx(e,t,n,o)}finally{Gn.transition=r,_t=o}return null}function Gx(e,t,n,o){do Ui();while(cr!==null);if((ot&6)!==0)throw Error(Q(327));n=e.finishedWork;var r=e.finishedLanes;if(n===null)return null;if(e.finishedWork=null,e.finishedLanes=0,n===e.current)throw Error(Q(177));e.callbackNode=null,e.callbackPriority=0;var i=n.lanes|n.childLanes;if(P2(e,i),e===Gt&&(zt=Gt=null,on=0),(n.subtreeFlags&2064)===0&&(n.flags&2064)===0||Ga||(Ga=!0,E0(cc,function(){return Ui(),null})),i=(n.flags&15990)!==0,(n.subtreeFlags&15990)!==0||i){i=Gn.transition,Gn.transition=null;var s=_t;_t=1;var l=ot;ot|=4,rm.current=null,Ux(e,n),h0(n,e),hx(t_),dc=!!e_,t_=e_=null,e.current=n,jx(n,e,r),b2(),ot=l,_t=s,Gn.transition=i}else e.current=n;if(Ga&&(Ga=!1,cr=e,Mc=r),i=e.pendingLanes,i===0&&(fr=null),C2(n.stateNode,o),Tn(e,Ot()),t!==null)for(o=e.onRecoverableError,n=0;n<t.length;n++)r=t[n],o(r.value,{componentStack:r.stack,digest:r.digest});if(Ec)throw Ec=!1,e=w_,w_=null,e;return(Mc&1)!==0&&e.tag!==0&&Ui(),i=e.pendingLanes,(i&1)!==0?e===b_?sl++:(sl=0,b_=e):sl=0,br(),null}function Ui(){if(cr!==null){var e=oh(Mc),t=Gn.transition,n=_t;try{if(Gn.transition=null,_t=16>e?16:e,cr===null)var o=!1;else{if(e=cr,cr=null,Mc=0,(ot&6)!==0)throw Error(Q(331));var r=ot;for(ot|=4,oe=e.current;oe!==null;){var i=oe,s=i.child;if((oe.flags&16)!==0){var l=i.deletions;if(l!==null){for(var a=0;a<l.length;a++){var c=l[a];for(oe=c;oe!==null;){var d=oe;switch(d.tag){case 0:case 11:case 15:rl(8,d,i)}var m=d.child;if(m!==null)m.return=d,oe=m;else for(;oe!==null;){d=oe;var f=d.sibling,k=d.return;if(p0(d),d===c){oe=null;break}if(f!==null){f.return=k,oe=f;break}oe=k}}}var w=i.alternate;if(w!==null){var C=w.child;if(C!==null){w.child=null;do{var S=C.sibling;C.sibling=null,C=S}while(C!==null)}}oe=i}}if((i.subtreeFlags&2064)!==0&&s!==null)s.return=i,oe=s;else e:for(;oe!==null;){if(i=oe,(i.flags&2048)!==0)switch(i.tag){case 0:case 11:case 15:rl(9,i,i.return)}var g=i.sibling;if(g!==null){g.return=i.return,oe=g;break e}oe=i.return}}var x=e.current;for(oe=x;oe!==null;){s=oe;var h=s.child;if((s.subtreeFlags&2064)!==0&&h!==null)h.return=s,oe=h;else e:for(s=x;oe!==null;){if(l=oe,(l.flags&2048)!==0)try{switch(l.tag){case 0:case 11:case 15:Oc(9,l)}}catch(O){$t(l,l.return,O)}if(l===s){oe=null;break e}var E=l.sibling;if(E!==null){E.return=l.return,oe=E;break e}oe=l.return}}if(ot=r,br(),Eo&&typeof Eo.onPostCommitFiberRoot=="function")try{Eo.onPostCommitFiberRoot(Rc,e)}catch{}o=!0}return o}finally{_t=n,Gn.transition=t}}return!1}function Rg(e,t,n){t=Gi(n,t),t=o0(e,t,1),e=pr(e,t,1),t=xn(),e!==null&&(Sl(e,1,t),Tn(e,t))}function $t(e,t,n){if(e.tag===3)Rg(e,e,n);else for(;t!==null;){if(t.tag===3){Rg(t,e,n);break}else if(t.tag===1){var o=t.stateNode;if(typeof t.type.getDerivedStateFromError=="function"||typeof o.componentDidCatch=="function"&&(fr===null||!fr.has(o))){e=Gi(n,e),e=r0(t,e,1),t=pr(t,e,1),e=xn(),t!==null&&(Sl(t,1,e),Tn(t,e));break}}t=t.return}}function qx(e,t,n){var o=e.pingCache;o!==null&&o.delete(t),t=xn(),e.pingedLanes|=e.suspendedLanes&n,Gt===e&&(on&n)===n&&(Yt===4||Yt===3&&(on&130023424)===on&&500>Ot()-sm?qr(e,0):im|=n),Tn(e,t)}function S0(e,t){t===0&&((e.mode&1)===0?t=1:(t=Aa,Aa<<=1,(Aa&130023424)===0&&(Aa=4194304)));var n=xn();e=Vo(e,t),e!==null&&(Sl(e,t,n),Tn(e,n))}function Kx(e){var t=e.memoizedState,n=0;t!==null&&(n=t.retryLane),S0(e,n)}function Jx(e,t){var n=0;switch(e.tag){case 13:var o=e.stateNode,r=e.memoizedState;r!==null&&(n=r.retryLane);break;case 19:o=e.stateNode;break;default:throw Error(Q(314))}o!==null&&o.delete(t),S0(e,n)}var C0;C0=function(e,t,n){if(e!==null)if(e.memoizedProps!==t.pendingProps||En.current)Cn=!0;else{if((e.lanes&n)===0&&(t.flags&128)===0)return Cn=!1,Bx(e,t,n);Cn=(e.flags&131072)!==0}else Cn=!1,Rt&&(t.flags&1048576)!==0&&Lh(t,yc,t.index);switch(t.lanes=0,t.tag){case 2:var o=t.type;nc(e,t),e=t.pendingProps;var r=Yi(t,pn.current);Wi(t,n),r=Z_(null,t,o,e,r,n);var i=em();return t.flags|=1,typeof r=="object"&&r!==null&&typeof r.render=="function"&&r.$$typeof===void 0?(t.tag=1,t.memoizedState=null,t.updateQueue=null,Mn(o)?(i=!0,gc(t)):i=!1,t.memoizedState=r.state!==null&&r.state!==void 0?r.state:null,Q_(t),r.updater=Fc,t.stateNode=r,r._reactInternals=t,u_(t,o,e,n),t=m_(null,t,o,!0,i,n)):(t.tag=0,Rt&&i&&H_(t),yn(null,t,r,n),t=t.child),t;case 16:o=t.elementType;e:{switch(nc(e,t),e=t.pendingProps,r=o._init,o=r(o._payload),t.type=o,r=t.tag=ev(o),e=so(o,e),r){case 0:t=__(null,t,o,e,n);break e;case 1:t=vg(null,t,o,e,n);break e;case 11:t=yg(null,t,o,e,n);break e;case 14:t=xg(null,t,o,so(o.type,e),n);break e}throw Error(Q(306,o,""))}return t;case 0:return o=t.type,r=t.pendingProps,r=t.elementType===o?r:so(o,r),__(e,t,o,r,n);case 1:return o=t.type,r=t.pendingProps,r=t.elementType===o?r:so(o,r),vg(e,t,o,r,n);case 3:e:{if(a0(t),e===null)throw Error(Q(387));o=t.pendingProps,i=t.memoizedState,r=i.element,Ah(e,t),wc(t,o,null,n);var s=t.memoizedState;if(o=s.element,i.isDehydrated)if(i={element:o,isDehydrated:!1,cache:s.cache,pendingSuspenseBoundaries:s.pendingSuspenseBoundaries,transitions:s.transitions},t.updateQueue.baseState=i,t.memoizedState=i,t.flags&256){r=Gi(Error(Q(423)),t),t=wg(e,t,o,n,r);break e}else if(o!==r){r=Gi(Error(Q(424)),t),t=wg(e,t,o,n,r);break e}else for($n=mr(t.stateNode.containerInfo.firstChild),Fn=t,Rt=!0,ao=null,n=Ih(t,null,o,n),t.child=n;n;)n.flags=n.flags&-3|4096,n=n.sibling;else{if(Vi(),o===r){t=Xo(e,t,n);break e}yn(e,t,o,n)}t=t.child}return t;case 5:return $h(t),e===null&&l_(t),o=t.type,r=t.pendingProps,i=e!==null?e.memoizedProps:null,s=r.children,n_(o,r)?s=null:i!==null&&n_(o,i)&&(t.flags|=32),l0(e,t),yn(e,t,s,n),t.child;case 6:return e===null&&l_(t),null;case 13:return c0(e,t,n);case 4:return G_(t,t.stateNode.containerInfo),o=t.pendingProps,e===null?t.child=Xi(t,null,o,n):yn(e,t,o,n),t.child;case 11:return o=t.type,r=t.pendingProps,r=t.elementType===o?r:so(o,r),yg(e,t,o,r,n);case 7:return yn(e,t,t.pendingProps,n),t.child;case 8:return yn(e,t,t.pendingProps.children,n),t.child;case 12:return yn(e,t,t.pendingProps.children,n),t.child;case 10:e:{if(o=t.type._context,r=t.pendingProps,i=t.memoizedProps,s=r.value,St(xc,o._currentValue),o._currentValue=s,i!==null)if(_o(i.value,s)){if(i.children===r.children&&!En.current){t=Xo(e,t,n);break e}}else for(i=t.child,i!==null&&(i.return=t);i!==null;){var l=i.dependencies;if(l!==null){s=i.child;for(var a=l.firstContext;a!==null;){if(a.context===o){if(i.tag===1){a=Uo(-1,n&-n),a.tag=2;var c=i.updateQueue;if(c!==null){c=c.shared;var d=c.pending;d===null?a.next=a:(a.next=d.next,d.next=a),c.pending=a}}i.lanes|=n,a=i.alternate,a!==null&&(a.lanes|=n),a_(i.return,n,t),l.lanes|=n;break}a=a.next}}else if(i.tag===10)s=i.type===t.type?null:i.child;else if(i.tag===18){if(s=i.return,s===null)throw Error(Q(341));s.lanes|=n,l=s.alternate,l!==null&&(l.lanes|=n),a_(s,n,t),s=i.sibling}else s=i.child;if(s!==null)s.return=i;else for(s=i;s!==null;){if(s===t){s=null;break}if(i=s.sibling,i!==null){i.return=s.return,s=i;break}s=s.return}i=s}yn(e,t,r.children,n),t=t.child}return t;case 9:return r=t.type,o=t.pendingProps.children,Wi(t,n),r=qn(r),o=o(r),t.flags|=1,yn(e,t,o,n),t.child;case 14:return o=t.type,r=so(o,t.pendingProps),r=so(o.type,r),xg(e,t,o,r,n);case 15:return i0(e,t,t.type,t.pendingProps,n);case 17:return o=t.type,r=t.pendingProps,r=t.elementType===o?r:so(o,r),nc(e,t),t.tag=1,Mn(o)?(e=!0,gc(t)):e=!1,Wi(t,n),n0(t,o,r),u_(t,o,r,n),m_(null,t,o,!0,e,n);case 19:return u0(e,t,n);case 22:return s0(e,t,n)}throw Error(Q(156,t.tag))};function E0(e,t){return Zg(e,t)}function Zx(e,t,n,o){this.tag=e,this.key=n,this.sibling=this.child=this.return=this.stateNode=this.type=this.elementType=null,this.index=0,this.ref=null,this.pendingProps=t,this.dependencies=this.memoizedState=this.updateQueue=this.memoizedProps=null,this.mode=o,this.subtreeFlags=this.flags=0,this.deletions=null,this.childLanes=this.lanes=0,this.alternate=null}function Qn(e,t,n,o){return new Zx(e,t,n,o)}function um(e){return e=e.prototype,!(!e||!e.isReactComponent)}function ev(e){if(typeof e=="function")return um(e)?1:0;if(e!=null){if(e=e.$$typeof,e===L_)return 11;if(e===R_)return 14}return 2}function hr(e,t){var n=e.alternate;return n===null?(n=Qn(e.tag,t,e.key,e.mode),n.elementType=e.elementType,n.type=e.type,n.stateNode=e.stateNode,n.alternate=e,e.alternate=n):(n.pendingProps=t,n.type=e.type,n.flags=0,n.subtreeFlags=0,n.deletions=null),n.flags=e.flags&14680064,n.childLanes=e.childLanes,n.lanes=e.lanes,n.child=e.child,n.memoizedProps=e.memoizedProps,n.memoizedState=e.memoizedState,n.updateQueue=e.updateQueue,t=e.dependencies,n.dependencies=t===null?null:{lanes:t.lanes,firstContext:t.firstContext},n.sibling=e.sibling,n.index=e.index,n.ref=e.ref,n}function ic(e,t,n,o,r,i){var s=2;if(o=e,typeof e=="function")um(e)&&(s=1);else if(typeof e=="string")s=5;else e:switch(e){case Mi:return Kr(n.children,r,i,t);case T_:s=8,r|=8;break;case Dd:return e=Qn(12,n,t,r|2),e.elementType=Dd,e.lanes=i,e;case Ad:return e=Qn(13,n,t,r),e.elementType=Ad,e.lanes=i,e;case $d:return e=Qn(19,n,t,r),e.elementType=$d,e.lanes=i,e;case $g:return zc(n,r,i,t);default:if(typeof e=="object"&&e!==null)switch(e.$$typeof){case Dg:s=10;break e;case Ag:s=9;break e;case L_:s=11;break e;case R_:s=14;break e;case rr:s=16,o=null;break e}throw Error(Q(130,e==null?e:typeof e,""))}return t=Qn(s,n,t,r),t.elementType=e,t.type=o,t.lanes=i,t}function Kr(e,t,n,o){return e=Qn(7,e,o,t),e.lanes=n,e}function zc(e,t,n,o){return e=Qn(22,e,o,t),e.elementType=$g,e.lanes=n,e.stateNode={isHidden:!1},e}function Pd(e,t,n){return e=Qn(6,e,null,t),e.lanes=n,e}function Nd(e,t,n){return t=Qn(4,e.children!==null?e.children:[],e.key,t),t.lanes=n,t.stateNode={containerInfo:e.containerInfo,pendingChildren:null,implementation:e.implementation},t}function tv(e,t,n,o,r){this.tag=t,this.containerInfo=e,this.finishedWork=this.pingCache=this.current=this.pendingChildren=null,this.timeoutHandle=-1,this.callbackNode=this.pendingContext=this.context=null,this.callbackPriority=0,this.eventTimes=fd(0),this.expirationTimes=fd(-1),this.entangledLanes=this.finishedLanes=this.mutableReadLanes=this.expiredLanes=this.pingedLanes=this.suspendedLanes=this.pendingLanes=0,this.entanglements=fd(0),this.identifierPrefix=o,this.onRecoverableError=r,this.mutableSourceEagerHydrationData=null}function dm(e,t,n,o,r,i,s,l,a){return e=new tv(e,t,n,l,a),t===1?(t=1,i===!0&&(t|=8)):t=0,i=Qn(3,null,null,t),e.current=i,i.stateNode=e,i.memoizedState={element:o,isDehydrated:n,cache:null,transitions:null,pendingSuspenseBoundaries:null},Q_(i),e}function nv(e,t,n){var o=3<arguments.length&&arguments[3]!==void 0?arguments[3]:null;return{$$typeof:Ei,key:o==null?null:""+o,children:e,containerInfo:t,implementation:n}}function M0(e){if(!e)return xr;e=e._reactInternals;e:{if(ri(e)!==e||e.tag!==1)throw Error(Q(170));var t=e;do{switch(t.tag){case 3:t=t.stateNode.context;break e;case 1:if(Mn(t.type)){t=t.stateNode.__reactInternalMemoizedMergedChildContext;break e}}t=t.return}while(t!==null);throw Error(Q(171))}if(e.tag===1){var n=e.type;if(Mn(n))return Mh(e,n,t)}return t}function T0(e,t,n,o,r,i,s,l,a){return e=dm(n,o,!0,e,r,i,s,l,a),e.context=M0(null),n=e.current,o=xn(),r=gr(n),i=Uo(o,r),i.callback=t??null,pr(n,i,r),e.current.lanes=r,Sl(e,r,o),Tn(e,o),e}function Hc(e,t,n,o){var r=t.current,i=xn(),s=gr(r);return n=M0(n),t.context===null?t.context=n:t.pendingContext=n,t=Uo(i,s),t.payload={element:e},o=o===void 0?null:o,o!==null&&(t.callback=o),e=pr(r,t,s),e!==null&&(uo(e,r,s,i),Za(e,r,s)),s}function Lc(e){return e=e.current,e.child?(e.child.tag===5,e.child.stateNode):null}function Pg(e,t){if(e=e.memoizedState,e!==null&&e.dehydrated!==null){var n=e.retryLane;e.retryLane=n!==0&&n<t?n:t}}function _m(e,t){Pg(e,t),(e=e.alternate)&&Pg(e,t)}function ov(){return null}var L0=typeof reportError=="function"?reportError:function(e){console.error(e)};function mm(e){this._internalRoot=e}Wc.prototype.render=mm.prototype.render=function(e){var t=this._internalRoot;if(t===null)throw Error(Q(409));Hc(e,t,null,null)};Wc.prototype.unmount=mm.prototype.unmount=function(){var e=this._internalRoot;if(e!==null){this._internalRoot=null;var t=e.containerInfo;ni(function(){Hc(null,e,null,null)}),t[Yo]=null}};function Wc(e){this._internalRoot=e}Wc.prototype.unstable_scheduleHydration=function(e){if(e){var t=sh();e={blockedOn:null,target:e,priority:t};for(var n=0;n<sr.length&&t!==0&&t<sr[n].priority;n++);sr.splice(n,0,e),n===0&&ah(e)}};function pm(e){return!(!e||e.nodeType!==1&&e.nodeType!==9&&e.nodeType!==11)}function Uc(e){return!(!e||e.nodeType!==1&&e.nodeType!==9&&e.nodeType!==11&&(e.nodeType!==8||e.nodeValue!==" react-mount-point-unstable "))}function Ng(){}function rv(e,t,n,o,r){if(r){if(typeof o=="function"){var i=o;o=function(){var c=Lc(s);i.call(c)}}var s=T0(t,o,e,0,null,!1,!1,"",Ng);return e._reactRootContainer=s,e[Yo]=s.current,fl(e.nodeType===8?e.parentNode:e),ni(),s}for(;r=e.lastChild;)e.removeChild(r);if(typeof o=="function"){var l=o;o=function(){var c=Lc(a);l.call(c)}}var a=dm(e,0,!1,null,null,!1,!1,"",Ng);return e._reactRootContainer=a,e[Yo]=a.current,fl(e.nodeType===8?e.parentNode:e),ni(function(){Hc(t,a,n,o)}),a}function jc(e,t,n,o,r){var i=n._reactRootContainer;if(i){var s=i;if(typeof r=="function"){var l=r;r=function(){var a=Lc(s);l.call(a)}}Hc(t,s,e,r)}else s=rv(n,t,e,r,o);return Lc(s)}rh=function(e){switch(e.tag){case 3:var t=e.stateNode;if(t.current.memoizedState.isDehydrated){var n=qs(t.pendingLanes);n!==0&&(I_(t,n|1),Tn(t,Ot()),(ot&6)===0&&(qi=Ot()+500,br()))}break;case 13:ni(function(){var o=Vo(e,1);if(o!==null){var r=xn();uo(o,e,1,r)}}),_m(e,1)}};D_=function(e){if(e.tag===13){var t=Vo(e,134217728);if(t!==null){var n=xn();uo(t,e,134217728,n)}_m(e,134217728)}};ih=function(e){if(e.tag===13){var t=gr(e),n=Vo(e,t);if(n!==null){var o=xn();uo(n,e,t,o)}_m(e,t)}};sh=function(){return _t};lh=function(e,t){var n=_t;try{return _t=e,t()}finally{_t=n}};Vd=function(e,t,n){switch(t){case"input":if(Bd(e,n),t=n.name,n.type==="radio"&&t!=null){for(n=e;n.parentNode;)n=n.parentNode;for(n=n.querySelectorAll("input[name="+JSON.stringify(""+t)+'][type="radio"]'),t=0;t<n.length;t++){var o=n[t];if(o!==e&&o.form===e.form){var r=Dc(o);if(!r)throw Error(Q(90));Og(o),Bd(o,r)}}}break;case"textarea":zg(e,n);break;case"select":t=n.value,t!=null&&Oi(e,!!n.multiple,t,!1)}};Xg=lm;Qg=ni;var iv={usingClientEntryPoint:!1,Events:[El,Pi,Dc,Yg,Vg,lm]},Vs={findFiberByHostInstance:Xr,bundleType:0,version:"18.3.1",rendererPackageName:"react-dom"},sv={bundleType:Vs.bundleType,version:Vs.version,rendererPackageName:Vs.rendererPackageName,rendererConfig:Vs.rendererConfig,overrideHookState:null,overrideHookStateDeletePath:null,overrideHookStateRenamePath:null,overrideProps:null,overridePropsDeletePath:null,overridePropsRenamePath:null,setErrorHandler:null,setSuspenseHandler:null,scheduleUpdate:null,currentDispatcherRef:Qo.ReactCurrentDispatcher,findHostInstanceByFiber:function(e){return e=Kg(e),e===null?null:e.stateNode},findFiberByHostInstance:Vs.findFiberByHostInstance||ov,findHostInstancesForRefresh:null,scheduleRefresh:null,scheduleRoot:null,setRefreshHandler:null,getCurrentFiber:null,reconcilerVersion:"18.3.1-next-f1338f8080-20240426"};if(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__<"u"&&(Xs=__REACT_DEVTOOLS_GLOBAL_HOOK__,!Xs.isDisabled&&Xs.supportsFiber))try{Rc=Xs.inject(sv),Eo=Xs}catch{}var Xs;zn.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED=iv;zn.createPortal=function(e,t){var n=2<arguments.length&&arguments[2]!==void 0?arguments[2]:null;if(!pm(t))throw Error(Q(200));return nv(e,t,null,n)};zn.createRoot=function(e,t){if(!pm(e))throw Error(Q(299));var n=!1,o="",r=L0;return t!=null&&(t.unstable_strictMode===!0&&(n=!0),t.identifierPrefix!==void 0&&(o=t.identifierPrefix),t.onRecoverableError!==void 0&&(r=t.onRecoverableError)),t=dm(e,1,!1,null,null,n,!1,o,r),e[Yo]=t.current,fl(e.nodeType===8?e.parentNode:e),new mm(t)};zn.findDOMNode=function(e){if(e==null)return null;if(e.nodeType===1)return e;var t=e._reactInternals;if(t===void 0)throw typeof e.render=="function"?Error(Q(188)):(e=Object.keys(e).join(","),Error(Q(268,e)));return e=Kg(t),e=e===null?null:e.stateNode,e};zn.flushSync=function(e){return ni(e)};zn.hydrate=function(e,t,n){if(!Uc(t))throw Error(Q(200));return jc(null,e,t,!0,n)};zn.hydrateRoot=function(e,t,n){if(!pm(e))throw Error(Q(405));var o=n!=null&&n.hydratedSources||null,r=!1,i="",s=L0;if(n!=null&&(n.unstable_strictMode===!0&&(r=!0),n.identifierPrefix!==void 0&&(i=n.identifierPrefix),n.onRecoverableError!==void 0&&(s=n.onRecoverableError)),t=T0(t,null,e,1,n??null,r,!1,i,s),e[Yo]=t.current,fl(e),o)for(e=0;e<o.length;e++)n=o[e],r=n._getVersion,r=r(n._source),t.mutableSourceEagerHydrationData==null?t.mutableSourceEagerHydrationData=[n,r]:t.mutableSourceEagerHydrationData.push(n,r);return new Wc(t)};zn.render=function(e,t,n){if(!Uc(t))throw Error(Q(200));return jc(null,e,t,!1,n)};zn.unmountComponentAtNode=function(e){if(!Uc(e))throw Error(Q(40));return e._reactRootContainer?(ni(function(){jc(null,null,e,!1,function(){e._reactRootContainer=null,e[Yo]=null})}),!0):!1};zn.unstable_batchedUpdates=lm;zn.unstable_renderSubtreeIntoContainer=function(e,t,n,o){if(!Uc(n))throw Error(Q(200));if(e==null||e._reactInternals===void 0)throw Error(Q(38));return jc(e,t,n,!1,o)};zn.version="18.3.1-next-f1338f8080-20240426"});var Tl=Oo((t4,N0)=>{"use strict";function P0(){if(!(typeof __REACT_DEVTOOLS_GLOBAL_HOOK__>"u"||typeof __REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE!="function"))try{__REACT_DEVTOOLS_GLOBAL_HOOK__.checkDCE(P0)}catch(e){console.error(e)}}P0(),N0.exports=R0()});var D0=Oo(fm=>{"use strict";var I0=Tl();fm.createRoot=I0.createRoot,fm.hydrateRoot=I0.hydrateRoot;var n4});var ty=Oo(Kc=>{"use strict";var vv=bt(),wv=Symbol.for("react.element"),bv=Symbol.for("react.fragment"),kv=Object.prototype.hasOwnProperty,Sv=vv.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED.ReactCurrentOwner,Cv={key:!0,ref:!0,__self:!0,__source:!0};function ey(e,t,n){var o,r={},i=null,s=null;n!==void 0&&(i=""+n),t.key!==void 0&&(i=""+t.key),t.ref!==void 0&&(s=t.ref);for(o in t)kv.call(t,o)&&!Cv.hasOwnProperty(o)&&(r[o]=t[o]);if(e&&e.defaultProps)for(o in t=e.defaultProps,t)r[o]===void 0&&(r[o]=t[o]);return{$$typeof:wv,type:e,key:i,ref:s,props:r,_owner:Sv.current}}Kc.Fragment=bv;Kc.jsx=ey;Kc.jsxs=ey});var Be=Oo((x4,ny)=>{"use strict";ny.exports=ty()});var Bk={};W5(Bk,{Agentation:()=>lp,createElement:()=>a5.createElement,createRoot:()=>c5.createRoot,version:()=>Ok});var a5=ae(bt()),c5=ae(D0());var Vc=ae(bt());var A0=["data-feedback-toolbar","data-annotation-popup","data-annotation-marker"],gm=A0.flatMap(e=>[`:not([${e}])`,`:not([${e}] *)`]).join(""),hm="feedback-freeze-styles",ym="__agentation_freeze";function lv(){return typeof window>"u"?{frozen:!1,installed:!0,origSetTimeout:setTimeout,origSetInterval:setInterval,origRAF:t=>0,pausedAnimations:[],frozenTimeoutQueue:[],frozenRAFQueue:[]}:window[ym]??{frozen:!1,installed:!1,origSetTimeout:window.setTimeout.bind(window),origSetInterval:window.setInterval.bind(window),origRAF:window.requestAnimationFrame.bind(window),pausedAnimations:[],frozenTimeoutQueue:[],frozenRAFQueue:[]}}var je=lv();function xm(){if(typeof window>"u")return;let e=window;je=e[ym]??(e[ym]=je),!je.installed&&(window.setTimeout=(t,n,...o)=>typeof t=="string"?je.origSetTimeout(t,n):je.origSetTimeout((...r)=>{je.frozen?je.frozenTimeoutQueue.push(()=>t(...r)):t(...r)},n,...o),window.setInterval=(t,n,...o)=>typeof t=="string"?je.origSetInterval(t,n):je.origSetInterval((...r)=>{je.frozen||t(...r)},n,...o),window.requestAnimationFrame=t=>je.origRAF(n=>{je.frozen?je.frozenRAFQueue.push(t):t(n)}),je.installed=!0)}var Ne=je.origSetTimeout,Yc=je.origSetInterval,Zi=je.origRAF;function av(e){return e?A0.some(t=>!!e.closest?.(`[${t}]`)):!1}function $0(){if(typeof document>"u"||(xm(),je.frozen))return;je.frozen=!0,je.frozenTimeoutQueue=[],je.frozenRAFQueue=[];let e=document.getElementById(hm);e||(e=document.createElement("style"),e.id=hm),e.textContent=`
    *${gm},
    *${gm}::before,
    *${gm}::after {
      animation-play-state: paused !important;
      transition: none !important;
    }
  `,document.head.appendChild(e),je.pausedAnimations=[];try{document.getAnimations().forEach(t=>{if(t.playState!=="running")return;let n=t.effect?.target;av(n)||(t.pause(),je.pausedAnimations.push(t))})}catch{}document.querySelectorAll("video").forEach(t=>{t.paused||(t.dataset.wasPaused="false",t.pause())})}function vm(){if(typeof document>"u"||!je.frozen)return;je.frozen=!1;let e=je.frozenTimeoutQueue;je.frozenTimeoutQueue=[];for(let n of e)je.origSetTimeout(()=>{if(je.frozen){je.frozenTimeoutQueue.push(n);return}try{n()}catch(o){console.warn("[agentation] Error replaying queued timeout:",o)}},0);let t=je.frozenRAFQueue;je.frozenRAFQueue=[];for(let n of t)je.origRAF(o=>{if(je.frozen){je.frozenRAFQueue.push(n);return}n(o)});for(let n of je.pausedAnimations)try{n.play()}catch(o){console.warn("[agentation] Error resuming animation:",o)}je.pausedAnimations=[],document.getElementById(hm)?.remove(),document.querySelectorAll("video").forEach(n=>{n.dataset.wasPaused==="false"&&(n.play().catch(()=>{}),delete n.dataset.wasPaused)})}function wm(){let e=(0,Vc.useMemo)(()=>{let t=0,n=new Set,o=()=>(n.forEach(clearTimeout),n.clear(),++t),r=s=>s===t;return{start:o,isCurrent:r,schedule:(s,l,a)=>{if(!r(s))return;let c=Ne(()=>{n.delete(c),r(s)&&l()},a);n.add(c)}}},[]);return(0,Vc.useEffect)(()=>()=>{e.start()},[e]),e}function F0(e,t,n,o){let r=c=>o.get(c.id)??c.id,i=new Map(e.map(c=>[r(c),c])),s=new Map(t.map(c=>[r(c),c])),l=new Set(n.map(c=>c.id)),a=[];for(let c of n){let d=i.get(c.id),m=s.get(c.id);if(d&&!m)continue;let f=m&&d&&m.comment!==d.comment;a.push(f?{...c,comment:m.comment}:c)}for(let[c,d]of s)!i.has(c)&&!l.has(c)&&a.push(d);return a}function es(e){if(e.tagName!=="IFRAME")return null;try{return e.contentDocument}catch{return null}}function wn(e,t=document){if(e===t)return null;try{return e.defaultView?.frameElement}catch{return null}}function Ll(e){return e.nodeType===11&&"host"in e}function ii(e){let t=e.getBoundingClientRect(),n=e.offsetWidth?t.width/e.offsetWidth:1,o=e.offsetHeight?t.height/e.offsetHeight:1,r=e.ownerDocument.defaultView?.getComputedStyle(e),i=d=>parseFloat(d||"0")||0,s=i(r?.paddingLeft),l=i(r?.paddingTop),a=e.clientWidth-s-i(r?.paddingRight),c=e.clientHeight-l-i(r?.paddingBottom);return{x:t.left+(e.clientLeft+s)*n,y:t.top+(e.clientTop+l)*o,sx:n,sy:o,width:a*n,height:c*o}}function O0(e){try{let t=new URL(e);return t.origin+t.pathname}catch{return e}}function cv(e,t,n,o=document){let r=wn(e,o);for(;r;){let i=ii(r);t=i.x+t*i.sx,n=i.y+n*i.sy,r=wn(r.ownerDocument,o)}return{x:t,y:n}}function Ft(e,t=document){let n=e.getBoundingClientRect();return B0(e.ownerDocument,n,t)}function B0(e,t,n){if(!wn(e,n))return t;let o=t.left,r=t.top,i=t.right,s=t.bottom,l=wn(e,n);for(;l;){let a=ii(l);o=Math.max(a.x,a.x+o*a.sx),r=Math.max(a.y,a.y+r*a.sy),i=Math.min(a.x+a.width,a.x+i*a.sx),s=Math.min(a.y+a.height,a.y+s*a.sy),l=wn(l.ownerDocument,n)}return new DOMRect(o,r,Math.max(0,i-o),Math.max(0,s-r))}function Xc(e){let t=[];for(let n of e.querySelectorAll("*"))n.tagName==="IFRAME"&&t.push(n),n.shadowRoot&&n.tagName!=="AGENTATION-TOOLBAR"&&t.push(...Xc(n.shadowRoot));return t}function z0(e,t,n,o=document){let r=[];for(let m=wn(e.ownerDocument,o);m;m=wn(m.ownerDocument,o))r.unshift(m);if(!r.length)return;let i=r.map(m=>{let f=ii(m);return t=(t-f.x)/f.sx,n=(n-f.y)/f.sy,{index:Xc(m.ownerDocument).indexOf(m),id:m.id||void 0,url:es(m)?.URL??""}}),s=e.ownerDocument.defaultView,l=!1;for(let m=e;m;m=m.parentElement)if(["fixed","sticky"].includes(s.getComputedStyle(m).position)){l=!0;break}let a=l?0:s.scrollX,c=l?0:s.scrollY,d=e.getBoundingClientRect();return{path:i,x:t+a,y:n+c,fixed:l,boundingBox:{x:d.left+a,y:d.top+c,width:d.width,height:d.height}}}function H0(e=document){let t=new Map,n=o=>{let r=t.get(o);return r||(r=Xc(o),t.set(o,r)),r};return o=>uv(o,e,n)}function uv(e,t=document,n=Xc){let o=e.frame;if(!o)return e;let r=t;for(let k of o.path){let w=n(r),C=k.id?w.find(g=>g.id===k.id):w[k.index],S=C&&es(C);if(!S||O0(S.URL)!==O0(k.url))return null;r=S}let i=r.defaultView,s=o.fixed?0:i.scrollX,l=o.fixed?0:i.scrollY,a=o.x-s,c=o.y-l;for(let k=r,w=wn(r,t);w;w=wn(k,t)){let C=k.defaultView;if(a<0||c<0||a>C.innerWidth||c>C.innerHeight)return null;let S=ii(w);if(S.width<=0||S.height<=0)return null;a=S.x+a*S.sx,c=S.y+c*S.sy,k=w.ownerDocument}let d=o.boundingBox,m=B0(r,new DOMRect(d.x-s,d.y-l,d.width,d.height),t),f=t.defaultView;return{...e,x:a/f.innerWidth*100,y:c+(e.isFixed?0:f.scrollY),boundingBox:{x:m.x,y:m.y+(e.isFixed?0:f.scrollY),width:m.width,height:m.height}}}function dv(e,t,n){if(!("clientX"in e)||t===n)return e;let o=cv(t,e.clientX,e.clientY,n);return new Proxy(e,{get(r,i){if(i==="clientX")return o.x;if(i==="clientY")return o.y;let s=Reflect.get(r,i,r);return typeof s=="function"?s.bind(r):s}})}function W0(e,t){let n=new Set([e]),o=new Set,r=[],i=new Set,s=!1,l=!1,a,c=(m,f)=>{let k=w=>m.listener(dv(w,f,e));m.handlers.set(f,k),f.addEventListener(m.type,k,m.options)},d=()=>{if(s=!1,!l)return;let m=new Set,f=new Set,k=[],w=C=>{k.push(C);for(let S of C.querySelectorAll("*"))if(!S.matches("agentation-toolbar, [data-agentation-portal]")&&(S.shadowRoot&&w(S.shadowRoot),S.tagName==="IFRAME")){f.add(S);let g=es(S);g&&!m.has(g)&&(m.add(g),w(g))}};m.add(e),w(e);for(let C of n)if(!m.has(C)){for(let S of o){let g=S.handlers.get(C);g&&C.removeEventListener(S.type,g,S.options),S.handlers.delete(C)}n.delete(C)}for(let C of m)if(!n.has(C)){n.add(C);for(let S of o)c(S,C)}for(let C of i)f.has(C)||C.removeEventListener("load",d);for(let C of f)i.has(C)||C.addEventListener("load",d);i=f,a?.disconnect(),i.forEach(C=>a?.observe(C)),r.forEach(C=>C.disconnect()),r=k.map(C=>{let S=new MutationObserver(g=>{g.some(h=>[...h.addedNodes,...h.removedNodes].some(E=>E.nodeType===1&&!E.closest("agentation-toolbar, [data-agentation-portal]")&&(E.tagName==="IFRAME"||!!E.shadowRoot||!!E.querySelector("iframe"))))&&!s&&(s=!0,queueMicrotask(d))});return S.observe(C,{childList:!0,subtree:!0}),S}),t?.()};return{start(){l||(l=!0,typeof ResizeObserver=="function"&&(a=new ResizeObserver(()=>t?.())),d())},stop(){l=!1,a?.disconnect(),a=void 0,r.forEach(m=>m.disconnect()),r=[];for(let m of i)m.removeEventListener("load",d);i.clear();for(let m of o)for(let[f,k]of m.handlers)f.removeEventListener(m.type,k,m.options);o.clear(),n.clear(),n.add(e)},addEventListener(m,f,k){let w={type:m,listener:f,options:k,handlers:new Map};o.add(w);for(let C of n)c(w,C)},removeEventListener(m,f,k){for(let w of o)if(w.type===m&&w.listener===f){for(let[C,S]of w.handlers)C.removeEventListener(m,S,w.options);o.delete(w)}},querySelectorAll(m){return[...n].flatMap(f=>[...f.querySelectorAll(m)])}}}var Qc=["data-testid","data-test","data-qa","data-cy","data-component"];function ts(e,t=Qc){let n={};for(let o of[...new Set(t)].slice(0,16)){if(!/^[a-zA-Z_][\w:.-]*$/.test(o))continue;let r=e.getAttribute(o);r!=null&&r.length<=500&&Object.defineProperty(n,o,{value:r,enumerable:!0})}return n}function U0(e,t=Qc){return Object.entries(ts(e,t)).filter(([n,o])=>/^data-[a-z0-9_-]+$/.test(n)&&o.length<=120).slice(0,2).map(([n,o])=>`[${n}="${o.replace(/[\\"\n\r\f\0]/g,r=>r==="\\"||r==='"'?`\\${r}`:`\\${r.charCodeAt(0).toString(16)} `)}"]`).join("")}function ns(e){if(e.parentElement)return e.parentElement;let t=e.getRootNode();return Ll(t)?t.host:null}function Vt(e,t){let n=e;for(;n;){if(n.matches(t))return n;n=ns(n)}return null}function j0(e,t=4,n){let o=[],r=e,i=0;for(;r&&i<t;){let l=r.tagName.toLowerCase();if(l==="html"||l==="body"){o.length===0&&o.push(l);break}let a=l;if(r.id)a=`#${r.id}`;else if(r.className&&typeof r.className=="string"){let d=r.className.split(/\s+/).find(m=>m.length>2&&!m.match(/^[a-z]{1,2}$/)&&!m.match(/[A-Z0-9]{5,}/));d&&(a=`.${d.split("_")[0]}`)}a+=U0(r,n);let c=ns(r);!r.parentElement&&c&&(a=`\u27E8shadow\u27E9 ${a}`),o.unshift(a),r=c,i++}let s=wn(e.ownerDocument);return(s?j0(s,2)+" > \u27E8iframe\u27E9 ":"")+o.join(" > ")}function _v(e){let t="";for(let n of e.childNodes)if(n.nodeType===Node.TEXT_NODE){let o=n.textContent?.trim();o&&(t+=(t?" ":"")+o)}return t}function kr(e,t){let n=j0(e,4,t);if(e.dataset.element)return{name:e.dataset.element,path:n};let o=e.tagName.toLowerCase();if(["path","circle","rect","line","g"].includes(o)){let r=Vt(e,"svg");if(r){let i=ns(r);if(i?.namespaceURI==="http://www.w3.org/1999/xhtml")return{name:`graphic in ${kr(i).name}`,path:n}}return{name:"graphic element",path:n}}if(o==="svg"){let r=ns(e);if(r?.tagName.toLowerCase()==="button"){let i=r.textContent?.trim();return{name:i?`icon in "${i}" button`:"button icon",path:n}}return{name:"icon",path:n}}if(o==="button"){let r=e.textContent?.trim(),i=e.getAttribute("aria-label");return i?{name:`button [${i}]`,path:n}:{name:r?`button "${r.slice(0,25)}"`:"button",path:n}}if(o==="a"){let r=e.textContent?.trim(),i=e.getAttribute("href");return r?{name:`link "${r.slice(0,25)}"`,path:n}:i?{name:`link to ${i.slice(0,30)}`,path:n}:{name:"link",path:n}}if(o==="input"){let r=e.getAttribute("type")||"text",i=e.getAttribute("placeholder"),s=e.getAttribute("name");return i?{name:`input "${i}"`,path:n}:s?{name:`input [${s}]`,path:n}:{name:`${r} input`,path:n}}if(["h1","h2","h3","h4","h5","h6"].includes(o)){let r=e.textContent?.trim();return{name:r?`${o} "${r.slice(0,35)}"`:o,path:n}}if(o==="p"){let r=e.textContent?.trim();return r?{name:`paragraph: "${r.slice(0,40)}${r.length>40?"...":""}"`,path:n}:{name:"paragraph",path:n}}if(o==="span"||o==="label"){let r=e.textContent?.trim();return r&&r.length<40?{name:`"${r}"`,path:n}:{name:o,path:n}}if(o==="li"){let r=e.textContent?.trim();return r&&r.length<40?{name:`list item: "${r.slice(0,35)}"`,path:n}:{name:"list item",path:n}}if(o==="blockquote")return{name:"blockquote",path:n};if(o==="code"){let r=e.textContent?.trim();return r&&r.length<30?{name:`code: \`${r}\``,path:n}:{name:"code",path:n}}if(o==="pre")return{name:"code block",path:n};if(o==="img"){let r=e.getAttribute("alt");return{name:r?`image "${r.slice(0,30)}"`:"image",path:n}}if(o==="video")return{name:"video",path:n};if(["div","section","article","nav","header","footer","aside","main"].includes(o)){let r=e.className,i=e.getAttribute("role"),s=e.getAttribute("aria-label");if(s)return{name:`${o} [${s}]`,path:n};if(i)return{name:`${i}`,path:n};let l=_v(e);if(l&&l.length<50)return{name:`"${l}"`,path:n};if(typeof r=="string"&&r){let a=r.split(/[\s_-]+/).map(c=>c.replace(/[A-Z0-9]{5,}.*$/,"")).filter(c=>c.length>2&&!/^[a-z]{1,2}$/.test(c)).slice(0,2);if(a.length>0)return{name:a.join(" "),path:n}}return{name:o==="div"?"container":o,path:n}}return{name:o,path:n}}function os(e){let t=[],n=e.textContent?.trim();n&&n.length<100&&t.push(n);let o=e.previousElementSibling;if(o){let i=o.textContent?.trim();i&&i.length<50&&t.unshift(`[before: "${i.slice(0,40)}"]`)}let r=e.nextElementSibling;if(r){let i=r.textContent?.trim();i&&i.length<50&&t.push(`[after: "${i.slice(0,40)}"]`)}return t.join(" ")}function Rl(e){let t=ns(e);if(!t)return"";let n=e.getRootNode(),r=(Ll(n)&&e.parentElement?Array.from(e.parentElement.children):Array.from(t.children)).filter(d=>d!==e&&d.namespaceURI==="http://www.w3.org/1999/xhtml");if(r.length===0)return"";let i=r.slice(0,4).map(d=>{let m=d.tagName.toLowerCase(),f=d.className,k="";if(typeof f=="string"&&f){let w=f.split(/\s+/).map(C=>C.replace(/[_][a-zA-Z0-9]{5,}.*$/,"")).find(C=>C.length>2&&!/^[a-z]{1,2}$/.test(C));w&&(k=`.${w}`)}if(m==="button"||m==="a"){let w=d.textContent?.trim().slice(0,15);if(w)return`${m}${k} "${w}"`}return`${m}${k}`}),l=t.tagName.toLowerCase();if(typeof t.className=="string"&&t.className){let d=t.className.split(/\s+/).map(m=>m.replace(/[_][a-zA-Z0-9]{5,}.*$/,"")).find(m=>m.length>2&&!/^[a-z]{1,2}$/.test(m));d&&(l=`.${d}`)}let a=t.children.length,c=a>i.length+1?` (${a} total in ${l})`:"";return i.join(", ")+c}function rs(e){let t=e.className;return typeof t!="string"||!t?"":t.split(/\s+/).filter(o=>o.length>0).map(o=>{let r=o.match(/^([a-zA-Z][a-zA-Z0-9_-]*?)(?:_[a-zA-Z0-9]{5,})?$/);return r?r[1]:o}).filter((o,r,i)=>i.indexOf(o)===r).join(", ")}var Y0=new Set(["none","normal","auto","0px","rgba(0, 0, 0, 0)","transparent","static","visible"]),mv=new Set(["p","span","h1","h2","h3","h4","h5","h6","label","li","td","th","blockquote","figcaption","caption","legend","dt","dd","pre","code","em","strong","b","i","a","time","cite","q"]),pv=new Set(["input","textarea","select"]),fv=new Set(["img","video","canvas","svg"]),gv=new Set(["div","section","article","nav","header","footer","aside","main","ul","ol","form","fieldset"]);function Pl(e){if(typeof window>"u")return{};let t=(e.ownerDocument.defaultView??window).getComputedStyle(e),n={},o=e.tagName.toLowerCase(),r;mv.has(o)?r=["color","fontSize","fontWeight","fontFamily","lineHeight"]:o==="button"||o==="a"&&e.getAttribute("role")==="button"?r=["backgroundColor","color","padding","borderRadius","fontSize"]:pv.has(o)?r=["backgroundColor","color","padding","borderRadius","fontSize"]:fv.has(o)?r=["width","height","objectFit","borderRadius"]:gv.has(o)?r=["display","padding","margin","gap","backgroundColor"]:r=["color","fontSize","margin","padding","backgroundColor"];for(let i of r){let s=i.replace(/([A-Z])/g,"-$1").toLowerCase(),l=t.getPropertyValue(s);l&&!Y0.has(l)&&(n[i]=l)}return n}var hv=["color","backgroundColor","borderColor","fontSize","fontWeight","fontFamily","lineHeight","letterSpacing","textAlign","width","height","padding","margin","border","borderRadius","display","position","top","right","bottom","left","zIndex","flexDirection","justifyContent","alignItems","gap","opacity","visibility","overflow","boxShadow","transform"];function Nl(e){if(typeof window>"u")return"";let t=(e.ownerDocument.defaultView??window).getComputedStyle(e),n=[];for(let o of hv){let r=o.replace(/([A-Z])/g,"-$1").toLowerCase(),i=t.getPropertyValue(r);i&&!Y0.has(i)&&n.push(`${r}: ${i}`)}return n.join("; ")}function V0(e){if(!e)return;let t={},n=e.split(";").map(o=>o.trim()).filter(Boolean);for(let o of n){let r=o.indexOf(":");if(r>0){let i=o.slice(0,r).trim(),s=o.slice(r+1).trim();i&&s&&(t[i]=s)}}return Object.keys(t).length>0?t:void 0}function Il(e){let t=[],n=e.getAttribute("role"),o=e.getAttribute("aria-label"),r=e.getAttribute("aria-describedby"),i=e.getAttribute("tabindex"),s=e.getAttribute("aria-hidden");return n&&t.push(`role="${n}"`),o&&t.push(`aria-label="${o}"`),r&&t.push(`aria-describedby="${r}"`),i&&t.push(`tabindex=${i}`),s==="true"&&t.push("aria-hidden"),e.matches("a, button, input, select, textarea, [tabindex]")&&t.push("focusable"),t.join(", ")}function is(e){let t=[],n=e;for(;n&&n.tagName.toLowerCase()!=="html";){let r=n.tagName.toLowerCase(),i=r;if(n.id)i=`${r}#${n.id}`;else if(n.className&&typeof n.className=="string"){let l=n.className.split(/\s+/).map(a=>a.replace(/[_][a-zA-Z0-9]{5,}.*$/,"")).find(a=>a.length>2);l&&(i=`${r}.${l}`)}let s=ns(n);!n.parentElement&&s&&(i=`\u27E8shadow\u27E9 ${i}`),t.unshift(i),n=s}let o=wn(e.ownerDocument);return(o?is(o)+" > \u27E8iframe\u27E9 ":"")+t.join(" > ")}var X0="agentation-toolbar, [data-agentation-root], [data-feedback-toolbar], [data-annotation-popup], [data-annotation-marker]",yv=new Set(["DIV","SPAN","SECTION","ARTICLE","MAIN","ASIDE","HEADER","FOOTER","NAV"]);function Dl(e,t){let n=document.elementFromPoint(e,t),o=new Set;for(;n&&!o.has(n);){o.add(n);let r=es(n),i;if(r){let s=ii(n);e=(e-s.x)/s.sx,t=(t-s.y)/s.sy,i=r.elementFromPoint?.(e,t)}else i=n.shadowRoot?.elementFromPoint?.(e,t);if(!i||i===n)break;n=i}return n}function xv(e){if(typeof e.checkVisibility=="function")return e.checkVisibility({checkOpacity:!0,checkVisibilityCSS:!0});let t=getComputedStyle(e);if(t.visibility==="hidden"||t.visibility==="collapse")return!1;let n=e;for(;n;){let o=getComputedStyle(n);if(o.opacity==="0"||o.display==="none"||o.contentVisibility==="hidden")return!1;let r=n.getRootNode();n=n.parentElement||(Ll(r)?r.host:null)}return!0}function Q0(e,t){let n=[],o=new Set,r=(i,s,l)=>{for(let a of i){if(o.has(a))continue;if(o.add(a),a.shadowRoot){let d=a.shadowRoot,m=d.elementsFromPoint?.(s,l)??[];r(m.length?m:[d.elementFromPoint?.(s,l)].filter(Boolean),s,l)}let c=es(a);if(c){let d=ii(a),m=(s-d.x)/d.sx,f=(l-d.y)/d.sy;r(c.elementsFromPoint?.(m,f)??[c.elementFromPoint?.(m,f)].filter(Boolean),m,f)}a!==a.ownerDocument.body&&a!==a.ownerDocument.documentElement&&!Vt(a,X0)&&xv(a)&&n.push(a)}};return r(document.elementsFromPoint?.(e,t)??[document.elementFromPoint(e,t)].filter(Boolean),e,t),n}function Al(e,t,n){if(n.width<=0||n.height<=0)return null;let o=null,r=1/0;for(let i of Q0(e,t)){let s=Ft(i),l=s.width/n.width,a=s.height/n.height;if(l<.5||l>2||a<.5||a>2)continue;let c=Math.abs(Math.log(l))+Math.abs(Math.log(a));c<r&&(o=i,r=c)}return o}function bm(e,t){let n=Dl(e,t);if(!n||Vt(n,X0))return null;let o=Q0(e,t);for(let s of o)if(!yv.has(s.tagName)&&!s.shadowRoot||Array.from(s.childNodes).some(l=>l.nodeType===Node.TEXT_NODE&&l.textContent?.trim()))return s;let r=null,i=1/0;for(let s of o){let l=Ft(s),a=l.width*l.height;a>0&&a<i&&(r=s,i=a)}return r}var M=ae(bt());var qc=ae(bt());function G0(e){let t=(0,qc.useCallback)(n=>e?(window.addEventListener("hashchange",n),window.addEventListener("popstate",n),()=>{window.removeEventListener("hashchange",n),window.removeEventListener("popstate",n)}):()=>{},[e]);return(0,qc.useSyncExternalStore)(t,()=>window.location.pathname+(e?window.location.hash:""),()=>"/")}var Gc=new Map;function q0(e,t){let n=(Gc.get(e)??Promise.resolve()).then(t),o=n.then(()=>{},()=>{});return Gc.set(e,o),o.then(()=>{Gc.get(e)===o&&Gc.delete(e)}),n}function K0(e,t,n){try{let o=new URL(e,n);return o.origin===n&&o.pathname+o.hash===t}catch{return!1}}var s5=ae(Tl());var ss=ae(bt()),J0=typeof window>"u"?ss.useEffect:ss.useLayoutEffect;function Z0(e){let[t,n]=(0,ss.useState)(null);return J0(()=>{let o=document.createElement("div");return o.setAttribute("data-agentation-portal",""),o.style.display="contents",n(o),()=>o.remove()},[]),J0(()=>{if(!t)return;let o=e??document.body;if(o.ownerDocument!==document){console.warn("[Agentation] portalContainer belongs to another document; the toolbar will not render.");return}let r=document.activeElement;for(;r?.shadowRoot?.activeElement;)r=r.shadowRoot.activeElement;let i=r&&t.contains(document.activeElement)?r:null;typeof t.hidePopover=="function"&&t.matches(":popover-open")&&t.hidePopover(),o.appendChild(t),e&&typeof t.showPopover=="function"?(t.setAttribute("popover","manual"),t.style.cssText="position:fixed;inset:0 auto auto 0;margin:0;padding:0;border:0;background:transparent;width:0;height:0;overflow:visible;pointer-events:none",t.showPopover()):(t.removeAttribute("popover"),t.style.cssText="display:contents"),i?.focus({preventScroll:!0})},[t,e]),t}var ls=ae(bt()),oy=ae(Tl()),iy=ae(Be()),ry=({mode:e="open",delegatesFocus:t,slotAssignment:n,host:o="div",children:r,className:i,...s})=>{let l=(0,ls.useRef)(null),[a,c]=(0,ls.useState)(null);return(0,ls.useLayoutEffect)(()=>{let m=l.current;if(!m||m.shadowRoot)return;let f=m.attachShadow({mode:e,delegatesFocus:t,slotAssignment:n});c(f)},[]),(0,iy.jsx)(o,{ref:l,...s,...o.includes("-")?{class:i}:{className:i},children:a&&(0,oy.createPortal)(r,a)})};var qt=ae(bt());var $l=ae(bt());function as(e,t,n){let o=(0,$l.useRef)(n);(0,$l.useLayoutEffect)(()=>{o.current=n},[n]),(0,$l.useLayoutEffect)(()=>{let r=e.current;if(!t||!r)return;let i=!1,s=r.getAnimations?.()??[];return Promise.allSettled(s.map(l=>l.finished)).then(()=>{i||o.current()}),()=>{i=!0}},[e,t])}var Jc=`@charset "UTF-8";
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
}`,Re={popup:"styles-module__popup___IhzrD",enter:"styles-module__enter___L7U7N",popupEnter:"styles-module__popupEnter___AuQDN",entered:"styles-module__entered___COX-w",exit:"styles-module__exit___5eGjE",popupExit:"styles-module__popupExit___JJKQX",shake:"styles-module__shake___jdbWe",header:"styles-module__header___wWsSi",element:"styles-module__element___fTV2z",headerToggle:"styles-module__headerToggle___WpW0b",chevron:"styles-module__chevron___ZZJlR",expanded:"styles-module__expanded___2Hxgv",stylesWrapper:"styles-module__stylesWrapper___pnHgy",stylesInner:"styles-module__stylesInner___YYZe2",stylesBlock:"styles-module__stylesBlock___VfQKn",styleLine:"styles-module__styleLine___1YQiD",styleProperty:"styles-module__styleProperty___84L1i",styleValue:"styles-module__styleValue___q51-h",timestamp:"styles-module__timestamp___Dtpsv",quote:"styles-module__quote___mcMmQ",screenshot:"styles-module__screenshot___7dYA7",textarea:"styles-module__textarea___jrSae",green:"styles-module__green___99l3h",actions:"styles-module__actions___D6x3f",sourceAction:"styles-module__sourceAction___EabJb",light:"styles-module__light___6AaSQ",cancel:"styles-module__cancel___hRjnL",submit:"styles-module__submit___K-mIR",deleteButton:"styles-module__deleteButton___4VuAE",deleteWrapper:"styles-module__deleteWrapper___oSjdo",sharedForm:"styles-module__sharedForm___8GvQl",previewExcerpt:"styles-module__previewExcerpt___DCOIL",sharedNote:"styles-module__sharedNote___OYVi5",sharedNoteContent:"styles-module__sharedNoteContent___6q3KD",sharedExtra:"styles-module__sharedExtra___RUBKC",sharedActions:"styles-module__sharedActions___6Glpl",sharedExtraInner:"styles-module__sharedExtraInner___EuUh4"};var sy=ae(bt()),km="data-agentation-styles";function Sm(e,t,n){if(!n||!e)return;let o=e.nodeType===9?e.head:e.nodeType===11?e:null;if(!o||typeof o.querySelector!="function"||o.querySelector(`style[${km}~="toolbar"], style[${km}~="${t}"]`))return;let i=(e.nodeType===9?e:e.ownerDocument).createElement("style");i.setAttribute(km,t),i.textContent=n,o.appendChild(i)}function Zc(e,t){return(0,sy.useCallback)(n=>{n&&Sm(n.getRootNode(),e,t)},[e,t])}var fn=ae(bt());var qe=ae(Be());function ly(e){if(!e)return;let t=n=>n.stopImmediatePropagation();document.addEventListener("focusin",t,!0),document.addEventListener("focusout",t,!0);try{e.focus({preventScroll:!0})}finally{document.removeEventListener("focusin",t,!0),document.removeEventListener("focusout",t,!0)}}var eu=(0,fn.forwardRef)(function({element:t,timestamp:n,selectedText:o,screenshot:r,placeholder:i="What should change?",initialValue:s="",submitLabel:l="Add",onSubmit:a,onCancel:c,onDelete:d,onOpenSource:m,allowEmpty:f=!1,accentColor:k="#3c82f7",computedStyles:w,disabled:C=!1,preview:S=!1,resetOnPreview:g=!0,variant:x="popup"},h){let E=x==="card",[O,j]=(0,fn.useState)(s),[A,Y]=(0,fn.useState)(!1),[K,q]=(0,fn.useState)(!1),Z=(0,fn.useRef)(null),Ie=(0,fn.useRef)(null),ye=o?` "${o.slice(0,30)}${o.length>30?"...":""}"`:"";(0,fn.useLayoutEffect)(()=>{let B=Ie.current,dt=Z.current;if(!E||!B||!dt)return;let ut=()=>{B.style.setProperty("--editor-field-height",`${dt.offsetHeight}px`)};if(ut(),"CanvasRenderingContext2D"in window){let $e=document.createElement("canvas").getContext("2d");if($e){let Fe=getComputedStyle(dt).fontFamily;$e.font=`13px ${Fe}`;let vt=$e.measureText(s.replace(/\s+/g," ")).width;$e.font=`italic 12px ${Fe}`;let Ct=$e.measureText(t+ye).width,Ht=Math.min(200,Math.max(120,Math.ceil(Math.max(vt,Ct))+24));B.closest("[data-annotation-card]")?.style.setProperty("--preview-width",`${Ht}px`);let Zt=B.querySelector("[data-shared-note]");Zt&&Zt.toggleAttribute("data-truncated",vt>Ht-24)}}let ue=typeof ResizeObserver<"u"?new ResizeObserver(ut):null;return ue?.observe(dt),()=>ue?.disconnect()},[E,t,s,ye]),(0,fn.useLayoutEffect)(()=>{S&&g&&(j(s),q(!1),Z.current&&(Z.current.scrollTop=0,Z.current.scrollLeft=0))},[S,g,s]),(0,fn.useImperativeHandle)(h,()=>({focus(){let B=Z.current;ly(B),B&&(B.selectionStart=B.selectionEnd=B.value.length,B.scrollTop=E?0:B.scrollHeight)}}),[E]);let me=(0,fn.useCallback)(()=>{C||!O.trim()&&!f||a(O.trim())},[C,O,f,a]),Me=B=>{B.stopPropagation(),!B.nativeEvent.isComposing&&(B.key==="Enter"&&!B.shiftKey&&(B.preventDefault(),me()),B.key==="Escape"&&c())};return(0,qe.jsxs)("div",{ref:Ie,className:E?Re.sharedForm:void 0,style:E?void 0:{display:"contents"},"data-annotation-editor":!0,"data-preview":S||void 0,children:[(0,qe.jsxs)("div",{className:Re.header,"data-editor-heading":!0,children:[w&&Object.keys(w).length>0?(0,qe.jsxs)("button",{className:Re.headerToggle,onClick:()=>{let B=K;q(!K),B&&Ne(()=>ly(Z.current),0)},type:"button",children:[(0,qe.jsx)("svg",{className:`${Re.chevron} ${K?Re.expanded:""}`,width:"14",height:"14",viewBox:"0 0 14 14",fill:"none",xmlns:"http://www.w3.org/2000/svg",children:(0,qe.jsx)("path",{d:"M5.5 10.25L9 7.25L5.75 4",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})}),(0,qe.jsxs)("span",{className:Re.element,children:[t,E&&ye&&(0,qe.jsx)("span",{className:Re.previewExcerpt,children:ye})]})]}):(0,qe.jsxs)("span",{className:Re.element,children:[t,E&&ye&&(0,qe.jsx)("span",{className:Re.previewExcerpt,children:ye})]}),n&&(0,qe.jsx)("span",{className:Re.timestamp,children:n})]}),m&&(0,qe.jsx)("div",{className:E?Re.sharedExtra:void 0,style:E?void 0:{display:"contents"},children:(0,qe.jsx)("div",{className:E?Re.sharedExtraInner:void 0,style:E?void 0:{display:"contents"},children:(0,qe.jsx)("button",{type:"button",className:Re.sourceAction,onClick:m,children:"Open in editor"})})}),w&&Object.keys(w).length>0&&(0,qe.jsx)("div",{className:`${Re.stylesWrapper} ${K?Re.expanded:""}`,children:(0,qe.jsx)("div",{className:Re.stylesInner,children:(0,qe.jsx)("div",{className:Re.stylesBlock,children:Object.entries(w).map(([B,dt])=>(0,qe.jsxs)("div",{className:Re.styleLine,children:[(0,qe.jsx)("span",{className:Re.styleProperty,children:B.replace(/([A-Z])/g,"-$1").toLowerCase()}),": ",(0,qe.jsx)("span",{className:Re.styleValue,children:dt}),";"]},B))})})}),o&&(0,qe.jsx)("div",{className:E?Re.sharedExtra:void 0,style:E?void 0:{display:"contents"},children:(0,qe.jsx)("div",{className:E?Re.sharedExtraInner:void 0,style:E?void 0:{display:"contents"},children:(0,qe.jsxs)("div",{className:Re.quote,children:["\u201C",o.slice(0,80),o.length>80?"...":"","\u201D"]})})}),r&&(0,qe.jsx)("img",{className:Re.screenshot,src:r,alt:"Screenshot sent with this note",title:"This picture is sent with the note",draggable:!1}),(0,qe.jsx)("div",{"data-shared-note":!0,className:E?Re.sharedNote:void 0,style:E?{"--field-border":A?k:void 0}:{display:"contents"},children:(0,qe.jsx)("div",{className:E?Re.sharedNoteContent:void 0,style:E?void 0:{display:"contents"},children:(0,qe.jsx)("textarea",{ref:Z,className:Re.textarea,readOnly:S,"aria-hidden":S,style:E?void 0:{borderColor:A?k:void 0},placeholder:i,value:S?O.replace(/\s+/g," "):O,onChange:B=>j(B.target.value),onFocus:()=>Y(!0),onBlur:()=>Y(!1),rows:2,onKeyDown:Me})})}),(0,qe.jsx)("div",{"data-editor-actions":!0,className:E?Re.sharedActions:void 0,style:E?void 0:{display:"contents"},children:(0,qe.jsxs)("div",{className:Re.actions,children:[d&&(0,qe.jsx)("div",{className:Re.deleteWrapper,children:(0,qe.jsx)("button",{className:Re.deleteButton,onClick:d,type:"button","aria-label":"Delete annotation",children:"Delete"})}),(0,qe.jsx)("button",{className:Re.cancel,onClick:c,children:"Cancel"}),(0,qe.jsx)("button",{className:Re.submit,style:{backgroundColor:k,opacity:O.trim()||f?1:.4},onClick:me,disabled:C||!O.trim()&&!f,children:l})]})})]})});var Cm=ae(Be()),cs=(0,qt.forwardRef)(function({element:t,timestamp:n,selectedText:o,screenshot:r,placeholder:i="What should change?",initialValue:s="",submitLabel:l="Add",onSubmit:a,onCancel:c,onDelete:d,onOpenSource:m,allowEmpty:f=!1,style:k,accentColor:w="#3c82f7",isExiting:C=!1,onExitComplete:S,lightMode:g=!1,computedStyles:x},h){let[E,O]=(0,qt.useState)(!1),[j,A]=(0,qt.useState)("initial"),Y=(0,qt.useRef)(null),K=(0,qt.useRef)(null);(0,qt.useEffect)(()=>{Sm(K.current?.getRootNode(),"annotation-popup",Jc)},[]);let q=(0,qt.useRef)(null);(0,qt.useEffect)(()=>{let Me=Ne(()=>{A(B=>B==="initial"?"enter":B)},0);return()=>{clearTimeout(Me),q.current&&clearTimeout(q.current)}},[]),(0,qt.useEffect)(()=>{if(C)return;let Me=Ne(()=>Y.current?.focus(),50);return()=>clearTimeout(Me)},[C]);let Z=(0,qt.useCallback)(()=>{q.current&&clearTimeout(q.current),O(!0),q.current=Ne(()=>{O(!1),Y.current?.focus()},250)},[]);(0,qt.useImperativeHandle)(h,()=>({shake:Z}),[Z]);let Ie=(0,qt.useCallback)(()=>{if(S){c();return}A("exit")},[c,S]),ye=C?"exit":j;as(K,ye==="exit",()=>{C?S?.():c()});let me=[Re.popup,g?Re.light:"",ye==="enter"?Re.enter:"",ye==="entered"?Re.entered:"",ye==="exit"?Re.exit:"",E&&ye!=="exit"?Re.shake:""].filter(Boolean).join(" ");return(0,Cm.jsx)("div",{ref:K,className:me,"data-annotation-popup":!0,style:k,onAnimationEnd:Me=>{Me.target===Me.currentTarget&&Me.animationName.includes("popupEnter")&&!C&&A("entered")},onKeyDownCapture:Me=>{Me.key!=="Escape"||Me.nativeEvent.isComposing||(Me.preventDefault(),Me.stopPropagation(),Ie())},onClick:Me=>Me.stopPropagation(),children:(0,Cm.jsx)(eu,{ref:Y,element:t,timestamp:n,selectedText:o,screenshot:r,placeholder:i,initialValue:s,submitLabel:l,onSubmit:a,onCancel:Ie,onDelete:d,onOpenSource:m,allowEmpty:f,accentColor:w,computedStyles:x,disabled:ye==="exit"})})});var us=`.icon-transitions-module__iconState___uqK9J {
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
}`,mt={iconState:"icon-transitions-module__iconState___uqK9J",iconStateFast:"icon-transitions-module__iconStateFast___HxlMm",iconFade:"icon-transitions-module__iconFade___nPwXg",iconFadeFast:"icon-transitions-module__iconFadeFast___Ofb2t",visible:"icon-transitions-module__visible___PlHsU",visibleScaled:"icon-transitions-module__visibleScaled___8Qog-",hidden:"icon-transitions-module__hidden___ETykt",hiddenScaled:"icon-transitions-module__hiddenScaled___JXn-m",sending:"icon-transitions-module__sending___uaLN-"};var te=ae(Be());var ay=({size:e=16})=>(0,te.jsx)("svg",{width:e,height:e,viewBox:"0 0 16 16",fill:"none",children:(0,te.jsx)("path",{d:"M8 3v10M3 8h10",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"})});var cy=({size:e=20,...t})=>(0,te.jsxs)("svg",{width:e,height:e,viewBox:"0 0 20 20",fill:"none",xmlns:"http://www.w3.org/2000/svg",...t,children:[(0,te.jsx)("circle",{cx:"10",cy:"10",r:"5.375",stroke:"currentColor",strokeWidth:"1.25"}),(0,te.jsx)("path",{d:"M8.5 8.5C8.73 7.85 9.31 7.49 10 7.5C10.86 7.51 11.5 8.13 11.5 9C11.5 10.08 10 10.5 10 10.5V10.75",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("circle",{cx:"10",cy:"12.625",r:"0.625",fill:"currentColor"})]});var uy=({size:e=24,copied:t=!1,tint:n})=>(0,te.jsxs)("svg",{ref:Zc("icon-transitions",us),width:e,height:e,viewBox:"0 0 24 24",fill:"none",style:n?{color:n,transition:"color 0.3s ease"}:void 0,children:[(0,te.jsxs)("g",{className:`${mt.iconState} ${t?mt.hiddenScaled:mt.visibleScaled}`,children:[(0,te.jsx)("path",{d:"M4.75 11.25C4.75 10.4216 5.42157 9.75 6.25 9.75H12.75C13.5784 9.75 14.25 10.4216 14.25 11.25V17.75C14.25 18.5784 13.5784 19.25 12.75 19.25H6.25C5.42157 19.25 4.75 18.5784 4.75 17.75V11.25Z",stroke:"currentColor",strokeWidth:"1.5"}),(0,te.jsx)("path",{d:"M17.25 14.25H17.75C18.5784 14.25 19.25 13.5784 19.25 12.75V6.25C19.25 5.42157 18.5784 4.75 17.75 4.75H11.25C10.4216 4.75 9.75 5.42157 9.75 6.25V6.75",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"})]}),(0,te.jsxs)("g",{className:`${mt.iconState} ${t?mt.visibleScaled:mt.hiddenScaled}`,children:[(0,te.jsx)("path",{d:"M12 20C7.58172 20 4 16.4182 4 12C4 7.58172 7.58172 4 12 4C16.4182 4 20 7.58172 20 12C20 16.4182 16.4182 20 12 20Z",stroke:"var(--agentation-color-green)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M15 10L11 14.25L9.25 12.25",stroke:"var(--agentation-color-green)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})]})]}),dy=({size:e=24,state:t="idle"})=>{let n=t==="idle",o=t==="sent",r=t==="failed",i=t==="sending";return(0,te.jsxs)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,te.jsx)("g",{className:`${mt.iconStateFast} ${n?mt.visibleScaled:i?mt.sending:mt.hiddenScaled}`,children:(0,te.jsx)("path",{d:"M9.875 14.125L12.3506 19.6951C12.7184 20.5227 13.9091 20.4741 14.2083 19.6193L18.8139 6.46032C19.0907 5.6695 18.3305 4.90933 17.5397 5.18611L4.38072 9.79174C3.52589 10.0909 3.47731 11.2816 4.30494 11.6494L9.875 14.125ZM9.875 14.125L13.375 10.625",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})}),(0,te.jsxs)("g",{className:`${mt.iconStateFast} ${o?mt.visibleScaled:mt.hiddenScaled}`,children:[(0,te.jsx)("path",{d:"M12 20C7.58172 20 4 16.4182 4 12C4 7.58172 7.58172 4 12 4C16.4182 4 20 7.58172 20 12C20 16.4182 16.4182 20 12 20Z",stroke:"var(--agentation-color-green)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M15 10L11 14.25L9.25 12.25",stroke:"var(--agentation-color-green)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})]}),(0,te.jsxs)("g",{className:`${mt.iconStateFast} ${r?mt.visibleScaled:mt.hiddenScaled}`,children:[(0,te.jsx)("path",{d:"M12 20C7.58172 20 4 16.4182 4 12C4 7.58172 7.58172 4 12 4C16.4182 4 20 7.58172 20 12C20 16.4182 16.4182 20 12 20Z",stroke:"var(--agentation-color-red)",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M12 8V12",stroke:"var(--agentation-color-red)",strokeWidth:"1.5",strokeLinecap:"round"}),(0,te.jsx)("circle",{cx:"12",cy:"15",r:"0.5",fill:"var(--agentation-color-red)",stroke:"var(--agentation-color-red)",strokeWidth:"1"})]})]})};var _y=({size:e=24,isOpen:t=!0})=>(0,te.jsxs)("svg",{ref:Zc("icon-transitions",us),width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,te.jsxs)("g",{className:`${mt.iconFade} ${t?mt.visible:mt.hidden}`,children:[(0,te.jsx)("path",{d:"M3.91752 12.7539C3.65127 12.2996 3.65037 11.7515 3.9149 11.2962C4.9042 9.59346 7.72688 5.49994 12 5.49994C16.2731 5.49994 19.0958 9.59346 20.0851 11.2962C20.3496 11.7515 20.3487 12.2996 20.0825 12.7539C19.0908 14.4459 16.2694 18.4999 12 18.4999C7.73064 18.4999 4.90918 14.4459 3.91752 12.7539Z",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M12 14.8261C13.5608 14.8261 14.8261 13.5608 14.8261 12C14.8261 10.4392 13.5608 9.17392 12 9.17392C10.4392 9.17392 9.17391 10.4392 9.17391 12C9.17391 13.5608 10.4392 14.8261 12 14.8261Z",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})]}),(0,te.jsxs)("g",{className:`${mt.iconFade} ${t?mt.hidden:mt.visible}`,children:[(0,te.jsx)("path",{d:"M18.6025 9.28503C18.9174 8.9701 19.4364 8.99481 19.7015 9.35271C20.1484 9.95606 20.4943 10.507 20.7342 10.9199C21.134 11.6086 21.1329 12.4454 20.7303 13.1328C20.2144 14.013 19.2151 15.5225 17.7723 16.8193C16.3293 18.1162 14.3852 19.2497 12.0008 19.25C11.4192 19.25 10.8638 19.1823 10.3355 19.0613C9.77966 18.934 9.63498 18.2525 10.0382 17.8493C10.2412 17.6463 10.5374 17.573 10.8188 17.6302C11.1993 17.7076 11.5935 17.75 12.0008 17.75C13.8848 17.7497 15.4867 16.8568 16.7693 15.7041C18.0522 14.5511 18.9606 13.1867 19.4363 12.375C19.5656 12.1543 19.5659 11.8943 19.4373 11.6729C19.2235 11.3049 18.921 10.8242 18.5364 10.3003C18.3085 9.98991 18.3302 9.5573 18.6025 9.28503ZM12.0008 4.75C12.5814 4.75006 13.1358 4.81803 13.6632 4.93953C14.2182 5.06741 14.362 5.74812 13.9593 6.15091C13.7558 6.35435 13.4589 6.42748 13.1771 6.36984C12.7983 6.29239 12.4061 6.25006 12.0008 6.25C10.1167 6.25 8.51415 7.15145 7.23028 8.31543C5.94678 9.47919 5.03918 10.8555 4.56426 11.6729C4.43551 11.8945 4.43582 12.1542 4.56524 12.375C4.77587 12.7343 5.07189 13.2012 5.44718 13.7105C5.67623 14.0213 5.65493 14.4552 5.38193 14.7282C5.0671 15.0431 4.54833 15.0189 4.28292 14.6614C3.84652 14.0736 3.50813 13.5369 3.27129 13.1328C2.86831 12.4451 2.86717 11.6088 3.26739 10.9199C3.78185 10.0345 4.77959 8.51239 6.22247 7.2041C7.66547 5.89584 9.61202 4.75 12.0008 4.75Z",fill:"currentColor"}),(0,te.jsx)("path",{d:"M5 19L19 5",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"})]})]}),my=({size:e=24,isPaused:t=!1})=>(0,te.jsxs)("svg",{ref:Zc("icon-transitions",us),width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,te.jsxs)("g",{className:`${mt.iconFadeFast} ${t?mt.hidden:mt.visible}`,children:[(0,te.jsx)("path",{d:"M8 6L8 18",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"}),(0,te.jsx)("path",{d:"M16 18L16 6",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round"})]}),(0,te.jsx)("path",{className:`${mt.iconFadeFast} ${t?mt.visible:mt.hidden}`,d:"M17.75 10.701C18.75 11.2783 18.75 12.7217 17.75 13.299L8.75 18.4952C7.75 19.0725 6.5 18.3509 6.5 17.1962L6.5 6.80384C6.5 5.64914 7.75 4.92746 8.75 5.50481L17.75 10.701Z",stroke:"currentColor",strokeWidth:"1.5"})]});var py=({size:e=16})=>(0,te.jsxs)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,te.jsx)("path",{d:"M10.6504 5.81117C10.9939 4.39628 13.0061 4.39628 13.3496 5.81117C13.5715 6.72517 14.6187 7.15891 15.4219 6.66952C16.6652 5.91193 18.0881 7.33479 17.3305 8.57815C16.8411 9.38134 17.2748 10.4285 18.1888 10.6504C19.6037 10.9939 19.6037 13.0061 18.1888 13.3496C17.2748 13.5715 16.8411 14.6187 17.3305 15.4219C18.0881 16.6652 16.6652 18.0881 15.4219 17.3305C14.6187 16.8411 13.5715 17.2748 13.3496 18.1888C13.0061 19.6037 10.9939 19.6037 10.6504 18.1888C10.4285 17.2748 9.38135 16.8411 8.57815 17.3305C7.33479 18.0881 5.91193 16.6652 6.66952 15.4219C7.15891 14.6187 6.72517 13.5715 5.81117 13.3496C4.39628 13.0061 4.39628 10.9939 5.81117 10.6504C6.72517 10.4285 7.15891 9.38134 6.66952 8.57815C5.91193 7.33479 7.33479 5.91192 8.57815 6.66952C9.38135 7.15891 10.4285 6.72517 10.6504 5.81117Z",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("circle",{cx:"12",cy:"12",r:"2.5",stroke:"currentColor",strokeWidth:"1.5"})]});var fy=({size:e=16})=>(0,te.jsx)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:(0,te.jsx)("path",{d:"M13.5 4C14.7426 4 15.75 5.00736 15.75 6.25V7H18.5C18.9142 7 19.25 7.33579 19.25 7.75C19.25 8.16421 18.9142 8.5 18.5 8.5H17.9678L17.6328 16.2217C17.61 16.7475 17.5912 17.1861 17.5469 17.543C17.5015 17.9087 17.4225 18.2506 17.2461 18.5723C16.9747 19.0671 16.5579 19.4671 16.0518 19.7168C15.7227 19.8791 15.3772 19.9422 15.0098 19.9717C14.6514 20.0004 14.2126 20 13.6865 20H10.3135C9.78735 20 9.34856 20.0004 8.99023 19.9717C8.62278 19.9422 8.27729 19.8791 7.94824 19.7168C7.44205 19.4671 7.02532 19.0671 6.75391 18.5723C6.57751 18.2506 6.49853 17.9087 6.45312 17.543C6.40883 17.1861 6.39005 16.7475 6.36719 16.2217L6.03223 8.5H5.5C5.08579 8.5 4.75 8.16421 4.75 7.75C4.75 7.33579 5.08579 7 5.5 7H8.25V6.25C8.25 5.00736 9.25736 4 10.5 4H13.5ZM7.86621 16.1562C7.89013 16.7063 7.90624 17.0751 7.94141 17.3584C7.97545 17.6326 8.02151 17.7644 8.06934 17.8516C8.19271 18.0763 8.38239 18.2577 8.6123 18.3711C8.70153 18.4151 8.83504 18.4545 9.11035 18.4766C9.39482 18.4994 9.76335 18.5 10.3135 18.5H13.6865C14.2367 18.5 14.6052 18.4994 14.8896 18.4766C15.165 18.4545 15.2985 18.4151 15.3877 18.3711C15.6176 18.2577 15.8073 18.0763 15.9307 17.8516C15.9785 17.7644 16.0245 17.6326 16.0586 17.3584C16.0938 17.0751 16.1099 16.7063 16.1338 16.1562L16.4668 8.5H7.5332L7.86621 16.1562ZM9.97656 10.75C10.3906 10.7371 10.7371 11.0626 10.75 11.4766L10.875 15.4766C10.8879 15.8906 10.5624 16.2371 10.1484 16.25C9.73443 16.2629 9.38794 15.9374 9.375 15.5234L9.25 11.5234C9.23706 11.1094 9.56255 10.7629 9.97656 10.75ZM14.0244 10.75C14.4384 10.7635 14.7635 11.1105 14.75 11.5244L14.6201 15.5244C14.6066 15.9384 14.2596 16.2634 13.8457 16.25C13.4317 16.2365 13.1067 15.8896 13.1201 15.4756L13.251 11.4756C13.2645 11.0617 13.6105 10.7366 14.0244 10.75ZM10.5 5.5C10.0858 5.5 9.75 5.83579 9.75 6.25V7H14.25V6.25C14.25 5.83579 13.9142 5.5 13.5 5.5H10.5Z",fill:"currentColor"})});var gy=({size:e=16})=>(0,te.jsxs)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,te.jsxs)("g",{clipPath:"url(#clip0_2_53)",children:[(0,te.jsx)("path",{d:"M16.25 16.25L7.75 7.75",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M7.75 16.25L16.25 7.75",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})]}),(0,te.jsx)("defs",{children:(0,te.jsx)("clipPath",{id:"clip0_2_53",children:(0,te.jsx)("rect",{width:"24",height:"24",fill:"white"})})})]});var hy=({size:e=16})=>(0,te.jsxs)("svg",{width:e,height:e,viewBox:"0 0 20 20",fill:"none",children:[(0,te.jsx)("path",{d:"M9.99999 12.7082C11.4958 12.7082 12.7083 11.4956 12.7083 9.99984C12.7083 8.50407 11.4958 7.2915 9.99999 7.2915C8.50422 7.2915 7.29166 8.50407 7.29166 9.99984C7.29166 11.4956 8.50422 12.7082 9.99999 12.7082Z",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M10 3.9585V5.05698",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M10 14.9429V16.0414",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M5.7269 5.72656L6.50682 6.50649",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M13.4932 13.4932L14.2731 14.2731",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M3.95834 10H5.05683",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M14.9432 10H16.0417",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M5.7269 14.2731L6.50682 13.4932",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"}),(0,te.jsx)("path",{d:"M13.4932 6.50649L14.2731 5.72656",stroke:"currentColor",strokeWidth:"1.25",strokeLinecap:"round",strokeLinejoin:"round"})]}),yy=({size:e=16})=>(0,te.jsx)("svg",{width:e,height:e,viewBox:"0 0 20 20",fill:"none",children:(0,te.jsx)("path",{d:"M15.5 10.4955C15.4037 11.5379 15.0124 12.5314 14.3721 13.3596C13.7317 14.1878 12.8688 14.8165 11.8841 15.1722C10.8995 15.5278 9.83397 15.5957 8.81217 15.3679C7.79038 15.1401 6.8546 14.6259 6.11434 13.8857C5.37408 13.1454 4.85995 12.2096 4.63211 11.1878C4.40427 10.166 4.47215 9.10048 4.82781 8.11585C5.18346 7.13123 5.81218 6.26825 6.64039 5.62791C7.4686 4.98756 8.46206 4.59634 9.5045 4.5C8.89418 5.32569 8.60049 6.34302 8.67685 7.36695C8.75321 8.39087 9.19454 9.35339 9.92058 10.0794C10.6466 10.8055 11.6091 11.2468 12.6331 11.3231C13.657 11.3995 14.6743 11.1058 15.5 10.4955Z",stroke:"currentColor",strokeWidth:"1.13793",strokeLinecap:"round",strokeLinejoin:"round"})}),xy=({size:e=16})=>(0,te.jsx)("svg",{width:e,height:e,viewBox:"0 0 16 16",fill:"none",xmlns:"http://www.w3.org/2000/svg",children:(0,te.jsx)("path",{d:"M11.3799 6.9572L9.05645 4.63375M11.3799 6.9572L6.74949 11.5699C6.61925 11.6996 6.45577 11.791 6.277 11.8339L4.29549 12.3092C3.93194 12.3964 3.60478 12.0683 3.69297 11.705L4.16585 9.75693C4.20893 9.57947 4.29978 9.4172 4.42854 9.28771L9.05645 4.63375M11.3799 6.9572L12.3455 5.98759C12.9839 5.34655 12.9839 4.31002 12.3455 3.66897C11.7033 3.02415 10.6594 3.02415 10.0172 3.66897L9.06126 4.62892L9.05645 4.63375",stroke:"currentColor",strokeWidth:"0.9",strokeLinecap:"round",strokeLinejoin:"round"})});var vy=({size:e=16})=>(0,te.jsx)("svg",{width:e,height:e,viewBox:"0 0 16 16",fill:"none",xmlns:"http://www.w3.org/2000/svg",children:(0,te.jsx)("path",{d:"M8.5 3.5L4 8L8.5 12.5",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})});var wy=({size:e=24})=>(0,te.jsxs)("svg",{width:e,height:e,viewBox:"0 0 24 24",fill:"none",children:[(0,te.jsx)("rect",{x:"3",y:"3",width:"18",height:"18",rx:"2",stroke:"currentColor",strokeWidth:"1.5"}),(0,te.jsx)("line",{x1:"3",y1:"9",x2:"21",y2:"9",stroke:"currentColor",strokeWidth:"1.5"}),(0,te.jsx)("line",{x1:"9",y1:"9",x2:"9",y2:"21",stroke:"currentColor",strokeWidth:"1.5"})]});var To=ae(bt()),by=ae(Tl());var si=ae(Be()),ky=({content:e,children:t,...n})=>{let[o,r]=(0,To.useState)(!1),[i,s]=(0,To.useState)(!1),[l,a]=(0,To.useState)({top:0,right:0}),c=(0,To.useRef)(null),d=(0,To.useRef)(null),m=(0,To.useRef)(null),f=()=>{if(c.current){let C=c.current.getBoundingClientRect();a({top:C.top+C.height/2,right:window.innerWidth-C.left+8})}},k=()=>{s(!0),m.current&&(clearTimeout(m.current),m.current=null),f(),d.current=Ne(()=>{r(!0)},500)},w=()=>{d.current&&(clearTimeout(d.current),d.current=null),r(!1),m.current=Ne(()=>{s(!1)},150)};return(0,To.useEffect)(()=>()=>{d.current&&clearTimeout(d.current),m.current&&clearTimeout(m.current)},[]),(0,si.jsxs)(si.Fragment,{children:[(0,si.jsx)("span",{ref:c,onMouseEnter:k,onMouseLeave:w,...n,children:t}),i&&(0,by.createPortal)((0,si.jsx)("div",{"data-feedback-toolbar":!0,style:{position:"fixed",top:l.top,right:l.right,transform:"translateY(-50%)",padding:"6px 10px",background:"#383838",color:"rgba(255, 255, 255, 0.7)",fontSize:"11px",fontWeight:400,lineHeight:"14px",borderRadius:"10px",width:"180px",textAlign:"left",zIndex:100020,pointerEvents:"none",boxShadow:"0px 1px 8px rgba(0, 0, 0, 0.28)",opacity:o?1:0,transition:"opacity 0.15s ease"},children:e}),document.body)]})};var Sy=`.styles-module__tooltip___mcXL2 {
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
}`,Em={tooltip:"styles-module__tooltip___mcXL2",tooltipIcon:"styles-module__tooltipIcon___Nq2nD"};var Mm=ae(Be()),Lo=({content:e})=>(0,Mm.jsx)(ky,{className:Em.tooltip,content:e,children:(0,Mm.jsx)(cy,{className:Em.tooltipIcon})});var Cy=`.styles-module__toolbar___wNsdK svg[fill=none],
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

.styles-module__hostPaletteAnchor___UFooW {
  position: fixed;
  top: 12px;
  right: 12px;
  width: 256px;
  z-index: 100000;
  pointer-events: none;
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
}`,H={toolbar:"styles-module__toolbar___wNsdK",markersLayer:"styles-module__markersLayer___-25j1",fixedMarkersLayer:"styles-module__fixedMarkersLayer___ffyX6",controlsContent:"styles-module__controlsContent___9GJWU",disableTransitions:"styles-module__disableTransitions___EopxO",positionContext:"styles-module__positionContext___AZFHE",hostPaletteAnchor:"styles-module__hostPaletteAnchor___UFooW",toolbarContainer:"styles-module__toolbarContainer___dIhma",entrance:"styles-module__entrance___sgHd8",toolbarEnter:"styles-module__toolbarEnter___u8RRu",hiding:"styles-module__hiding___1td44",toolbarHide:"styles-module__toolbarHide___y8kaT",collapsed:"styles-module__collapsed___Rydsn",expanded:"styles-module__expanded___ofKPx",serverConnected:"styles-module__serverConnected___Gfbou",buttonWrapper:"styles-module__buttonWrapper___rBcdv",toggleWrapper:"styles-module__toggleWrapper___7N0-q",togglePlaceholder:"styles-module__togglePlaceholder___wnqrL",toggleContent:"styles-module__toggleContent___0yfyP",expandedToggle:"styles-module__expandedToggle___F7SRN",toggleGlyph:"styles-module__toggleGlyph___R7Oom",toggleIcon:"styles-module__toggleIcon___Jbtus",toggleTopLine:"styles-module__toggleTopLine___hQaCm",toggleMiddleLine:"styles-module__toggleMiddleLine___sFFVe",toggleBottomLine:"styles-module__toggleBottomLine___V-jX3",toggleSparkle:"styles-module__toggleSparkle___eeF99",controlButton:"styles-module__controlButton___8Q0jc",visible:"styles-module__visible___KHwEW",hidden:"styles-module__hidden___Ae8H4",badge:"styles-module__badge___2XsgF",fadeOut:"styles-module__fadeOut___6Ut6-",badgeEnter:"styles-module__badgeEnter___mVQLj",statusShowing:"styles-module__statusShowing___te6iu",buttonBadge:"styles-module__buttonBadge___NeFWb",mcpIndicator:"styles-module__mcpIndicator___zGJeL",connected:"styles-module__connected___7c28g",mcpIndicatorPulseConnected:"styles-module__mcpIndicatorPulseConnected___EDodZ",connecting:"styles-module__connecting___uo-CW",mcpIndicatorPulseConnecting:"styles-module__mcpIndicatorPulseConnecting___cCYte",connectionIndicatorWrapper:"styles-module__connectionIndicatorWrapper___L-e-3",connectionIndicator:"styles-module__connectionIndicator___afk9p",connectionIndicatorVisible:"styles-module__connectionIndicatorVisible___C-i5B",connectionIndicatorConnected:"styles-module__connectionIndicatorConnected___IY8pR",connectionPulse:"styles-module__connectionPulse___-Zycw",connectionIndicatorDisconnected:"styles-module__connectionIndicatorDisconnected___kmpaZ",connectionIndicatorConnecting:"styles-module__connectionIndicatorConnecting___QmSLH",buttonTooltip:"styles-module__buttonTooltip___Burd9",tooltipsInSession:"styles-module__tooltipsInSession___-0lHH",sendButtonWrapper:"styles-module__sendButtonWrapper___UUxG6",sendButtonVisible:"styles-module__sendButtonVisible___WPSQU",shortcut:"styles-module__shortcut___lEAQk",tooltipBelow:"styles-module__tooltipBelow___m6ats",tooltipsHidden:"styles-module__tooltipsHidden___VtLJG",tooltipVisible:"styles-module__tooltipVisible___0jcCv",buttonWrapperAlignLeft:"styles-module__buttonWrapperAlignLeft___myzIp",buttonWrapperAlignRight:"styles-module__buttonWrapperAlignRight___HCQFR",divider:"styles-module__divider___c--s1",overlay:"styles-module__overlay___Q1O9y",hoverHighlight:"styles-module__hoverHighlight___ogakW",enter:"styles-module__enter___WFIki",hoverHighlightIn:"styles-module__hoverHighlightIn___6WYHY",multiSelectOutline:"styles-module__multiSelectOutline___cSJ-m",fadeIn:"styles-module__fadeIn___b9qmf",exit:"styles-module__exit___fyOJ0",singleSelectOutline:"styles-module__singleSelectOutline___QhX-O",hoverTooltip:"styles-module__hoverTooltip___bvLk7",hoverTooltipIn:"styles-module__hoverTooltipIn___FYGQx",hoverReactPath:"styles-module__hoverReactPath___gx1IJ",hoverElementName:"styles-module__hoverElementName___QMLMl",marker:"styles-module__marker___6sQrs",clearing:"styles-module__clearing___FQ--7",markerIn:"styles-module__markerIn___5FaAP",markerOut:"styles-module__markerOut___GU5jX",pending:"styles-module__pending___2IHLC",fixed:"styles-module__fixed___dBMHC",multiSelect:"styles-module__multiSelect___YWiuz",hovered:"styles-module__hovered___ZgXIy",renumber:"styles-module__renumber___nCTxD",renumberRoll:"styles-module__renumberRoll___Wgbq3",markerTooltip:"styles-module__markerTooltip___aLJID",tooltipIn:"styles-module__tooltipIn___0N31w",markerQuote:"styles-module__markerQuote___FHmrz",markerNote:"styles-module__markerNote___QkrrS",markerHint:"styles-module__markerHint___2iF-6",settingsPanel:"styles-module__settingsPanel___OxX3Y",settingsHeader:"styles-module__settingsHeader___pwDY9",settingsBrand:"styles-module__settingsBrand___0gJeM",settingsBrandSlash:"styles-module__settingsBrandSlash___uTG18",settingsVersion:"styles-module__settingsVersion___TUcFq",settingsSection:"styles-module__settingsSection___m-YM2",settingsLabel:"styles-module__settingsLabel___8UjfX",cycleButton:"styles-module__cycleButton___FMKfw",cycleDot:"styles-module__cycleDot___nPgLY",dropdownButton:"styles-module__dropdownButton___16NPz",toggleLabel:"styles-module__toggleLabel___Xm8Aa",customCheckbox:"styles-module__customCheckbox___U39ax",sliderLabel:"styles-module__sliderLabel___U8sPr",slider:"styles-module__slider___GLdxp",themeToggle:"styles-module__themeToggle___2rUjA",settingsOption:"styles-module__settingsOption___UNa12",selected:"styles-module__selected___OwRqP",settingsPanelContainer:"styles-module__settingsPanelContainer___Xksv8",settingsPage:"styles-module__settingsPage___6YfHH",slideLeft:"styles-module__slideLeft___Ps01J",automationsPage:"styles-module__automationsPage___uvCq6",slideIn:"styles-module__slideIn___4-qXe",settingsNavLink:"styles-module__settingsNavLink___wCzJt",settingsNavLinkRight:"styles-module__settingsNavLinkRight___ZWwhj",mcpNavIndicator:"styles-module__mcpNavIndicator___cl9pO",mcpPulse:"styles-module__mcpPulse___uNggr",settingsBackButton:"styles-module__settingsBackButton___bIe2j",automationHeader:"styles-module__automationHeader___InP0r",automationDescription:"styles-module__automationDescription___NKlmo",learnMoreLink:"styles-module__learnMoreLink___8xv-x",autoSendRow:"styles-module__autoSendRow___UblX5",autoSendLabel:"styles-module__autoSendLabel___icDc2",active:"styles-module__active___-zoN6",webhookUrlInput:"styles-module__webhookUrlInput___2375C",settingsSectionExtraPadding:"styles-module__settingsSectionExtraPadding___jdhFV",settingsSectionGrow:"styles-module__settingsSectionGrow___h-5HZ",settingsRow:"styles-module__settingsRow___3sdhc",settingsRowMarginTop:"styles-module__settingsRowMarginTop___zA0Sp",dropdownContainer:"styles-module__dropdownContainer___BVnxe",settingsRowDisabled:"styles-module__settingsRowDisabled___EgS0V",toggleSwitch:"styles-module__toggleSwitch___l4Ygm",cycleButtonText:"styles-module__cycleButtonText___fD1LR",cycleTextIn:"styles-module__cycleTextIn___Q6zJf",cycleDots:"styles-module__cycleDots___LWuoQ",dropdownMenu:"styles-module__dropdownMenu___k73ER",scaleIn:"styles-module__scaleIn___c-r1K",dropdownItem:"styles-module__dropdownItem___ylsLj",settingsLabelMarker:"styles-module__settingsLabelMarker___ewdtV",settingsOptions:"styles-module__settingsOptions___LyrBA",sliderContainer:"styles-module__sliderContainer___ducXj",sliderLabels:"styles-module__sliderLabels___FhLDB",colorOptions:"styles-module__colorOptions___iHCNX",colorOption:"styles-module__colorOption___IodiY",colorOptionRing:"styles-module__colorOptionRing___U2xpo",settingsToggle:"styles-module__settingsToggle___fBrFn",settingsToggleMarginBottom:"styles-module__settingsToggleMarginBottom___MZUyF",mcpStatusDot:"styles-module__mcpStatusDot___ibgkc",disconnected:"styles-module__disconnected___cHPxR",mcpPulseError:"styles-module__mcpPulseError___fov9B",drawCanvas:"styles-module__drawCanvas___7cG9U",dragSelection:"styles-module__dragSelection___kZLq2",dragCount:"styles-module__dragCount___KM90j",highlightsContainer:"styles-module__highlightsContainer___-0xzG",selectedElementHighlight:"styles-module__selectedElementHighlight___fyVlI",scaleOut:"styles-module__scaleOut___Wctwz",slideUp:"styles-module__slideUp___kgD36",slideDown:"styles-module__slideDown___zcdje"};var li=ae(Be());function Ey({active:e}){return(0,li.jsxs)("svg",{className:H.toggleGlyph,"data-active":e,width:"24",height:"24",viewBox:"0 0 24 24",fill:"none",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round","aria-hidden":"true",children:[(0,li.jsx)("path",{className:H.toggleTopLine,d:"M5.5 6.75H18.5"}),(0,li.jsx)("path",{className:H.toggleMiddleLine,d:"M5.5 12H11.5"}),(0,li.jsx)("path",{className:H.toggleBottomLine,d:"M5.5 17.25H9.25"}),(0,li.jsx)("path",{className:H.toggleSparkle,d:"M16 12.75L16.5179 13.9677C16.8078 14.6494 17.3506 15.1922 18.0323 15.4821L19.25 16L18.0323 16.5179C17.3506 16.8078 16.8078 17.3506 16.5179 18.0323L16 19.25L15.4821 18.0323C15.1922 17.3506 14.6494 16.8078 13.9677 16.5179L12.75 16L13.9677 15.4821C14.6494 15.1922 15.1922 14.6494 15.4821 13.9677L16 12.75Z"})]})}var lt=ae(bt());var ee={navigation:{width:800,height:56},hero:{width:800,height:320},header:{width:800,height:80},section:{width:800,height:400},sidebar:{width:240,height:400},footer:{width:800,height:160},modal:{width:480,height:300},card:{width:280,height:240},text:{width:400,height:120},image:{width:320,height:200},video:{width:480,height:270},table:{width:560,height:220},grid:{width:600,height:300},list:{width:300,height:180},chart:{width:400,height:240},button:{width:140,height:40},input:{width:280,height:56},form:{width:360,height:320},tabs:{width:480,height:240},dropdown:{width:200,height:200},toggle:{width:44,height:24},search:{width:320,height:44},avatar:{width:48,height:48},badge:{width:80,height:28},breadcrumb:{width:300,height:24},pagination:{width:300,height:36},progress:{width:240,height:8},divider:{width:600,height:1},accordion:{width:400,height:200},carousel:{width:600,height:300},toast:{width:320,height:64},tooltip:{width:180,height:40},pricing:{width:300,height:360},testimonial:{width:360,height:200},cta:{width:600,height:160},alert:{width:400,height:56},banner:{width:800,height:48},stat:{width:200,height:120},stepper:{width:480,height:48},tag:{width:72,height:28},rating:{width:160,height:28},map:{width:480,height:300},timeline:{width:360,height:320},fileUpload:{width:360,height:180},codeBlock:{width:480,height:200},calendar:{width:300,height:300},notification:{width:360,height:72},productCard:{width:280,height:360},profile:{width:280,height:200},drawer:{width:320,height:400},popover:{width:240,height:160},logo:{width:120,height:40},faq:{width:560,height:320},gallery:{width:560,height:360},checkbox:{width:20,height:20},radio:{width:20,height:20},slider:{width:240,height:32},datePicker:{width:300,height:320},skeleton:{width:320,height:120},chip:{width:96,height:32},icon:{width:24,height:24},spinner:{width:32,height:32},feature:{width:360,height:200},team:{width:560,height:280},login:{width:360,height:360},contact:{width:400,height:320}},Tm=[{section:"Layout",items:[{type:"navigation",label:"Navigation",...ee.navigation},{type:"header",label:"Header",...ee.header},{type:"hero",label:"Hero",...ee.hero},{type:"section",label:"Section",...ee.section},{type:"sidebar",label:"Sidebar",...ee.sidebar},{type:"footer",label:"Footer",...ee.footer},{type:"modal",label:"Modal",...ee.modal},{type:"banner",label:"Banner",...ee.banner},{type:"drawer",label:"Drawer",...ee.drawer},{type:"popover",label:"Popover",...ee.popover},{type:"divider",label:"Divider",...ee.divider}]},{section:"Content",items:[{type:"card",label:"Card",...ee.card},{type:"text",label:"Text",...ee.text},{type:"image",label:"Image",...ee.image},{type:"video",label:"Video",...ee.video},{type:"table",label:"Table",...ee.table},{type:"grid",label:"Grid",...ee.grid},{type:"list",label:"List",...ee.list},{type:"chart",label:"Chart",...ee.chart},{type:"codeBlock",label:"Code Block",...ee.codeBlock},{type:"map",label:"Map",...ee.map},{type:"timeline",label:"Timeline",...ee.timeline},{type:"calendar",label:"Calendar",...ee.calendar},{type:"accordion",label:"Accordion",...ee.accordion},{type:"carousel",label:"Carousel",...ee.carousel},{type:"logo",label:"Logo",...ee.logo},{type:"faq",label:"FAQ",...ee.faq},{type:"gallery",label:"Gallery",...ee.gallery}]},{section:"Controls",items:[{type:"button",label:"Button",...ee.button},{type:"input",label:"Input",...ee.input},{type:"search",label:"Search",...ee.search},{type:"form",label:"Form",...ee.form},{type:"tabs",label:"Tabs",...ee.tabs},{type:"dropdown",label:"Dropdown",...ee.dropdown},{type:"toggle",label:"Toggle",...ee.toggle},{type:"stepper",label:"Stepper",...ee.stepper},{type:"rating",label:"Rating",...ee.rating},{type:"fileUpload",label:"File Upload",...ee.fileUpload},{type:"checkbox",label:"Checkbox",...ee.checkbox},{type:"radio",label:"Radio",...ee.radio},{type:"slider",label:"Slider",...ee.slider},{type:"datePicker",label:"Date Picker",...ee.datePicker}]},{section:"Elements",items:[{type:"avatar",label:"Avatar",...ee.avatar},{type:"badge",label:"Badge",...ee.badge},{type:"tag",label:"Tag",...ee.tag},{type:"breadcrumb",label:"Breadcrumb",...ee.breadcrumb},{type:"pagination",label:"Pagination",...ee.pagination},{type:"progress",label:"Progress",...ee.progress},{type:"alert",label:"Alert",...ee.alert},{type:"toast",label:"Toast",...ee.toast},{type:"notification",label:"Notification",...ee.notification},{type:"tooltip",label:"Tooltip",...ee.tooltip},{type:"stat",label:"Stat",...ee.stat},{type:"skeleton",label:"Skeleton",...ee.skeleton},{type:"chip",label:"Chip",...ee.chip},{type:"icon",label:"Icon",...ee.icon},{type:"spinner",label:"Spinner",...ee.spinner}]},{section:"Blocks",items:[{type:"pricing",label:"Pricing",...ee.pricing},{type:"testimonial",label:"Testimonial",...ee.testimonial},{type:"cta",label:"CTA",...ee.cta},{type:"productCard",label:"Product Card",...ee.productCard},{type:"profile",label:"Profile",...ee.profile},{type:"feature",label:"Feature",...ee.feature},{type:"team",label:"Team",...ee.team},{type:"login",label:"Login",...ee.login},{type:"contact",label:"Contact",...ee.contact}]}],Hn={};for(let e of Tm)for(let t of e.items)Hn[t.type]=t;var _=ae(Be());function W({w:e,h:t=3,strong:n}){return(0,_.jsx)("div",{style:{width:typeof e=="number"?`${e}px`:e,height:t,borderRadius:2,background:n?"var(--agd-bar-strong)":"var(--agd-bar)",flexShrink:0}})}function pt({w:e,h:t,radius:n=3,style:o}){return(0,_.jsx)("div",{style:{width:typeof e=="number"?`${e}px`:e,height:typeof t=="number"?`${t}px`:t,borderRadius:n,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",flexShrink:0,...o}})}function bn({size:e}){return(0,_.jsx)("div",{style:{width:e,height:e,borderRadius:"50%",border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",flexShrink:0}})}function Ev({width:e,height:t}){let n=Math.max(8,t*.2);return(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",height:"100%",padding:`0 ${n}px`,gap:e*.02},children:[(0,_.jsx)(pt,{w:Math.max(20,t*.5),h:Math.max(12,t*.4),radius:2}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",gap:e*.03,marginLeft:e*.04},children:[(0,_.jsx)(W,{w:e*.06}),(0,_.jsx)(W,{w:e*.07}),(0,_.jsx)(W,{w:e*.05}),(0,_.jsx)(W,{w:e*.06})]}),(0,_.jsx)(pt,{w:e*.1,h:Math.min(28,t*.5),radius:4})]})}function Mv({width:e,height:t,text:n}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",height:"100%",gap:t*.05},children:[n?(0,_.jsx)("span",{style:{fontSize:Math.min(20,t*.08),fontWeight:600,color:"var(--agd-text-3)",textAlign:"center",maxWidth:"80%"},children:n}):(0,_.jsx)(W,{w:e*.5,h:Math.max(6,t*.04),strong:!0}),(0,_.jsx)(W,{w:e*.6}),(0,_.jsx)(W,{w:e*.4}),(0,_.jsx)(pt,{w:Math.min(140,e*.2),h:Math.min(36,t*.12),radius:6,style:{marginTop:t*.06}})]})}function Tv({width:e,height:t}){let n=Math.max(3,Math.floor(t/36));return(0,_.jsxs)("div",{style:{padding:e*.08,display:"flex",flexDirection:"column",gap:t*.03},children:[(0,_.jsx)(W,{w:e*.6,h:4,strong:!0}),Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:6},children:[(0,_.jsx)(pt,{w:10,h:10,radius:2}),(0,_.jsx)(W,{w:e*(.4+r*17%30/100)})]},r))]})}function Lv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/160)));return(0,_.jsx)("div",{style:{display:"flex",padding:`${t*.12}px ${e*.03}px`,gap:e*.05},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsx)(W,{w:"60%",h:3,strong:!0}),(0,_.jsx)(W,{w:"80%",h:2}),(0,_.jsx)(W,{w:"70%",h:2}),(0,_.jsx)(W,{w:"60%",h:2})]},r))})}function Rv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsxs)("div",{style:{padding:"10px 12px",borderBottom:"1px solid var(--agd-stroke)",display:"flex",alignItems:"center",justifyContent:"space-between"},children:[(0,_.jsx)(W,{w:e*.3,h:4,strong:!0}),(0,_.jsx)("div",{style:{width:14,height:14,border:"1px solid var(--agd-stroke)",borderRadius:3}})]}),(0,_.jsxs)("div",{style:{flex:1,padding:12,display:"flex",flexDirection:"column",gap:6},children:[(0,_.jsx)(W,{w:"90%"}),(0,_.jsx)(W,{w:"70%"}),(0,_.jsx)(W,{w:"80%"})]}),(0,_.jsxs)("div",{style:{padding:"10px 12px",borderTop:"1px solid var(--agd-stroke)",display:"flex",justifyContent:"flex-end",gap:8},children:[(0,_.jsx)(pt,{w:70,h:26,radius:4}),(0,_.jsx)(pt,{w:70,h:26,radius:4,style:{background:"var(--agd-bar)"}})]})]})}function Pv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{height:"40%",background:"var(--agd-fill)",borderBottom:"1px dashed var(--agd-stroke)"}}),(0,_.jsxs)("div",{style:{flex:1,padding:10,display:"flex",flexDirection:"column",gap:5},children:[(0,_.jsx)(W,{w:"70%",h:4,strong:!0}),(0,_.jsx)(W,{w:"95%",h:2}),(0,_.jsx)(W,{w:"85%",h:2}),(0,_.jsx)(W,{w:"50%",h:2})]})]})}function Nv({width:e,height:t,text:n}){if(n)return(0,_.jsx)("div",{style:{padding:4,fontSize:Math.min(14,t*.3),lineHeight:1.5,color:"var(--agd-text-3)",wordBreak:"break-word",overflow:"hidden"},children:n});let o=Math.max(2,Math.floor(t/18));return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:6,padding:4},children:[(0,_.jsx)(W,{w:e*.6,h:5,strong:!0}),Array.from({length:o},(r,i)=>(0,_.jsx)(W,{w:`${70+i*13%25}%`,h:2},i))]})}function Iv({width:e,height:t}){return(0,_.jsx)("div",{style:{height:"100%",position:"relative"},children:(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,preserveAspectRatio:"none",fill:"none",children:[(0,_.jsx)("line",{x1:"0",y1:"0",x2:e,y2:t,stroke:"var(--agd-stroke)",strokeWidth:"1"}),(0,_.jsx)("line",{x1:e,y1:"0",x2:"0",y2:t,stroke:"var(--agd-stroke)",strokeWidth:"1"}),(0,_.jsx)("circle",{cx:e*.3,cy:t*.3,r:Math.min(e,t)*.08,fill:"var(--agd-fill)",stroke:"var(--agd-stroke)",strokeWidth:"0.8"})]})})}function Dv({width:e,height:t}){let n=Math.max(2,Math.min(5,Math.floor(e/100))),o=Math.max(2,Math.min(6,Math.floor(t/32)));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{display:"flex",borderBottom:"1px solid var(--agd-stroke)",padding:"6px 0"},children:Array.from({length:n},(r,i)=>(0,_.jsx)("div",{style:{flex:1,padding:"0 8px"},children:(0,_.jsx)(W,{w:"70%",h:3,strong:!0})},i))}),Array.from({length:o},(r,i)=>(0,_.jsx)("div",{style:{display:"flex",borderBottom:"1px solid rgba(255,255,255,0.03)",padding:"6px 0"},children:Array.from({length:n},(s,l)=>(0,_.jsx)("div",{style:{flex:1,padding:"0 8px"},children:(0,_.jsx)(W,{w:`${50+(i*7+l*13)%40}%`,h:2})},l))},i))]})}function Av({width:e,height:t}){let n=Math.max(2,Math.floor(t/28));return(0,_.jsx)("div",{style:{display:"flex",flexDirection:"column",gap:4,padding:4},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:8,padding:"4px 0"},children:[(0,_.jsx)(bn,{size:8}),(0,_.jsx)(W,{w:`${55+r*17%35}%`,h:2})]},r))})}function $v({width:e,height:t,text:n}){return(0,_.jsx)("div",{style:{height:"100%",borderRadius:Math.min(8,t/3),border:"1px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center"},children:n?(0,_.jsx)("span",{style:{fontSize:Math.min(13,t*.4),fontWeight:500,color:"var(--agd-text-3)",letterSpacing:"-0.01em"},children:n}):(0,_.jsx)(W,{w:Math.max(20,e*.5),h:3,strong:!0})})}function Fv({width:e,height:t}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:4,height:"100%",justifyContent:"center"},children:[(0,_.jsx)(W,{w:Math.min(80,e*.3),h:2}),(0,_.jsx)("div",{style:{height:Math.min(36,t*.6),borderRadius:4,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",paddingLeft:8},children:(0,_.jsx)(W,{w:"40%",h:2})})]})}function Ov({width:e,height:t}){let n=Math.max(2,Math.min(5,Math.floor(t/56)));return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:t*.04,padding:8},children:[Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsx)(W,{w:60+r*17%30,h:2}),(0,_.jsx)(pt,{w:"100%",h:28,radius:4})]},r)),(0,_.jsx)(pt,{w:Math.min(120,e*.35),h:30,radius:6,style:{marginTop:8,alignSelf:"flex-end",background:"var(--agd-bar)"}})]})}function Bv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/120)));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{display:"flex",gap:2,borderBottom:"1px solid var(--agd-stroke)"},children:Array.from({length:n},(o,r)=>(0,_.jsx)("div",{style:{padding:"8px 12px",borderBottom:r===0?"2px solid var(--agd-bar-strong)":"none"},children:(0,_.jsx)(W,{w:60,h:3,strong:r===0})},r))}),(0,_.jsxs)("div",{style:{flex:1,padding:12,display:"flex",flexDirection:"column",gap:6},children:[(0,_.jsx)(W,{w:"80%",h:2}),(0,_.jsx)(W,{w:"65%",h:2}),(0,_.jsx)(W,{w:"75%",h:2})]})]})}function zv({width:e,height:t}){let n=Math.min(e,t)/2;return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("circle",{cx:e/2,cy:t/2,r:n-1,stroke:"var(--agd-stroke)",fill:"var(--agd-fill)",strokeWidth:"1.5",strokeDasharray:"3 2"}),(0,_.jsx)("circle",{cx:e/2,cy:t*.38,r:n*.28,stroke:"var(--agd-stroke)",fill:"var(--agd-fill)",strokeWidth:"0.8"}),(0,_.jsx)("path",{d:`M${e/2-n*.55} ${t*.78} C${e/2-n*.55} ${t*.55} ${e/2+n*.55} ${t*.55} ${e/2+n*.55} ${t*.78}`,stroke:"var(--agd-stroke)",fill:"var(--agd-fill)",strokeWidth:"0.8"})]})}function Hv({width:e,height:t}){return(0,_.jsx)("div",{style:{height:"100%",borderRadius:t/2,border:"1px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)(W,{w:Math.max(16,e*.5),h:2,strong:!0})})}function Wv({width:e,height:t}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",height:"100%",gap:t*.08},children:[(0,_.jsx)(W,{w:e*.5,h:Math.max(5,t*.06),strong:!0}),(0,_.jsx)(W,{w:e*.35})]})}function Uv({width:e,height:t}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",height:"100%",gap:t*.04,padding:e*.04},children:[(0,_.jsx)(W,{w:e*.3,h:4,strong:!0}),(0,_.jsx)(W,{w:e*.7}),(0,_.jsx)(W,{w:e*.5}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",gap:e*.03,marginTop:t*.06},children:[(0,_.jsx)(pt,{w:"33%",h:"100%",radius:4}),(0,_.jsx)(pt,{w:"33%",h:"100%",radius:4}),(0,_.jsx)(pt,{w:"33%",h:"100%",radius:4})]})]})}function jv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/140))),o=Math.max(1,Math.min(3,Math.floor(t/120)));return(0,_.jsx)("div",{style:{display:"grid",gridTemplateColumns:`repeat(${n}, 1fr)`,gridTemplateRows:`repeat(${o}, 1fr)`,gap:6,height:"100%"},children:Array.from({length:n*o},(r,i)=>(0,_.jsx)(pt,{w:"100%",h:"100%",radius:4},i))})}function Yv({width:e,height:t}){let n=Math.max(2,Math.floor((t-32)/28));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{padding:"6px 8px",borderBottom:"1px solid var(--agd-stroke)"},children:(0,_.jsx)(W,{w:e*.5,h:3,strong:!0})}),(0,_.jsx)("div",{style:{flex:1,padding:4,display:"flex",flexDirection:"column",gap:2},children:Array.from({length:n},(o,r)=>(0,_.jsx)("div",{style:{padding:"4px 6px",borderRadius:3,background:r===0?"var(--agd-fill)":"transparent"},children:(0,_.jsx)(W,{w:`${50+r*17%35}%`,h:2,strong:r===0})},r))})]})}function Vv({width:e,height:t}){let n=Math.min(e,t)/2;return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("rect",{x:"1",y:"1",width:e-2,height:t-2,rx:n,stroke:"var(--agd-stroke)",strokeWidth:"1"}),(0,_.jsx)("circle",{cx:e-n,cy:t/2,r:n*.7,fill:"var(--agd-bar)"})]})}function Xv({width:e,height:t}){let n=Math.min(t/2,20);return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:n,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:`0 ${n*.6}px`,gap:6},children:[(0,_.jsx)(bn,{size:Math.min(14,t*.4)}),(0,_.jsx)(W,{w:"50%",h:2})]})}function Qv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:8,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:"0 10px",gap:8},children:[(0,_.jsx)(bn,{size:Math.min(20,t*.5)}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(W,{w:"60%",h:3,strong:!0}),(0,_.jsx)(W,{w:"80%",h:2})]}),(0,_.jsx)("div",{style:{width:14,height:14,border:"1px solid var(--agd-stroke)",borderRadius:3,flexShrink:0}})]})}function Gv({width:e,height:t}){return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("rect",{x:"0",y:"0",width:e,height:t,rx:t/2,stroke:"var(--agd-stroke)",strokeWidth:"0.8"}),(0,_.jsx)("rect",{x:"1",y:"1",width:e*.65,height:t-2,rx:(t-2)/2,fill:"var(--agd-bar)"})]})}function qv({width:e,height:t}){let n=Math.max(3,Math.min(7,Math.floor(e/50))),o=e/(n*2);return(0,_.jsx)("div",{style:{height:"100%",display:"flex",alignItems:"flex-end",justifyContent:"space-around",padding:"0 4px",borderBottom:"1px solid var(--agd-stroke)"},children:Array.from({length:n},(r,i)=>{let s=30+(i*37+17)%55;return(0,_.jsx)(pt,{w:o,h:`${s}%`,radius:2},i)})})}function Kv({width:e,height:t}){let n=Math.min(e,t)*.12;return(0,_.jsxs)("div",{style:{height:"100%",position:"relative",display:"flex",alignItems:"center",justifyContent:"center"},children:[(0,_.jsx)(pt,{w:"100%",h:"100%",radius:4}),(0,_.jsx)("div",{style:{position:"absolute",width:n*2,height:n*2,borderRadius:"50%",border:"1.5px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)("div",{style:{width:0,height:0,borderLeft:`${n*.6}px solid var(--agd-bar-strong)`,borderTop:`${n*.4}px solid transparent`,borderBottom:`${n*.4}px solid transparent`,marginLeft:n*.15}})})]})}function Jv({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center"},children:[(0,_.jsx)("div",{style:{flex:1,width:"100%",borderRadius:6,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)(W,{w:"60%",h:2})}),(0,_.jsx)("div",{style:{width:8,height:8,background:"var(--agd-fill)",border:"1px dashed var(--agd-stroke)",borderTop:"none",borderLeft:"none",transform:"rotate(45deg)",marginTop:-5}})]})}function Zv({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/80)));return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",height:"100%",gap:4},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:4},children:[r>0&&(0,_.jsx)("span",{style:{color:"var(--agd-stroke)",fontSize:10},children:"/"}),(0,_.jsx)(W,{w:40+r*13%20,h:2,strong:r===n-1})]},r))})}function ew({width:e,height:t}){let n=Math.max(3,Math.min(5,Math.floor(e/40))),o=Math.min(28,t*.8);return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:"100%",gap:4},children:Array.from({length:n},(r,i)=>(0,_.jsx)(pt,{w:o,h:o,radius:4,style:i===1?{background:"var(--agd-bar)"}:void 0},i))})}function tw({width:e}){return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",height:"100%"},children:(0,_.jsx)("div",{style:{width:"100%",height:1,background:"var(--agd-stroke)"}})})}function nw({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(t/40)));return(0,_.jsx)("div",{style:{display:"flex",flexDirection:"column",height:"100%"},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{borderBottom:"1px solid var(--agd-stroke)",padding:"8px 6px",display:"flex",alignItems:"center",justifyContent:"space-between",flex:r===0?2:1},children:[(0,_.jsx)(W,{w:`${40+r*17%25}%`,h:3,strong:!0}),(0,_.jsx)("span",{style:{fontSize:8,color:"var(--agd-stroke)"},children:r===0?"\u25BC":"\u25B6"})]},r))})}function ow({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",gap:6},children:[(0,_.jsxs)("div",{style:{flex:1,display:"flex",gap:6,alignItems:"center"},children:[(0,_.jsx)("span",{style:{fontSize:12,color:"var(--agd-stroke)"},children:"\u2039"}),(0,_.jsx)(pt,{w:"100%",h:"100%",radius:4}),(0,_.jsx)("span",{style:{fontSize:12,color:"var(--agd-stroke)"},children:"\u203A"})]}),(0,_.jsxs)("div",{style:{display:"flex",justifyContent:"center",gap:4},children:[(0,_.jsx)(bn,{size:5}),(0,_.jsx)(bn,{size:5}),(0,_.jsx)(bn,{size:5})]})]})}function rw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",padding:10,gap:t*.04},children:[(0,_.jsx)(W,{w:e*.4,h:3,strong:!0}),(0,_.jsx)(W,{w:e*.3,h:6,strong:!0}),(0,_.jsx)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:4,width:"100%",padding:"8px 0"},children:Array.from({length:4},(n,o)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:4},children:[(0,_.jsx)(bn,{size:5}),(0,_.jsx)(W,{w:`${50+o*17%35}%`,h:2})]},o))}),(0,_.jsx)(pt,{w:e*.7,h:Math.min(32,t*.1),radius:6,style:{background:"var(--agd-bar)"}})]})}function iw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",padding:10,gap:8},children:[(0,_.jsx)("span",{style:{fontSize:18,lineHeight:1,color:"var(--agd-stroke)",fontFamily:"serif"},children:"\u201C"}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsx)(W,{w:"90%",h:2}),(0,_.jsx)(W,{w:"75%",h:2}),(0,_.jsx)(W,{w:"60%",h:2})]}),(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:6},children:[(0,_.jsx)(bn,{size:20}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:2},children:[(0,_.jsx)(W,{w:60,h:3,strong:!0}),(0,_.jsx)(W,{w:40,h:2})]})]})]})}function sw({width:e,height:t}){return(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",height:"100%",gap:t*.08},children:[(0,_.jsx)(W,{w:e*.5,h:Math.max(4,t*.05),strong:!0}),(0,_.jsx)(W,{w:e*.35}),(0,_.jsx)(pt,{w:Math.min(140,e*.25),h:Math.min(32,t*.15),radius:6,style:{marginTop:t*.04,background:"var(--agd-bar)"}})]})}function lw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:6,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:"0 10px",gap:8},children:[(0,_.jsx)("div",{style:{width:16,height:16,borderRadius:"50%",border:"1.5px solid var(--agd-bar-strong)",display:"flex",alignItems:"center",justifyContent:"center",flexShrink:0},children:(0,_.jsx)("div",{style:{width:2,height:6,background:"var(--agd-bar-strong)",borderRadius:1}})}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(W,{w:"40%",h:3,strong:!0}),(0,_.jsx)(W,{w:"70%",h:2})]})]})}function aw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center",gap:8,padding:"0 12px"},children:[(0,_.jsx)(W,{w:e*.4,h:3,strong:!0}),(0,_.jsx)(pt,{w:60,h:Math.min(24,t*.6),radius:4})]})}function cw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",gap:t*.06},children:[(0,_.jsx)(W,{w:e*.5,h:2}),(0,_.jsx)(W,{w:e*.4,h:Math.max(8,t*.18),strong:!0}),(0,_.jsx)(W,{w:e*.3,h:2})]})}function uw({width:e,height:t}){let n=Math.max(3,Math.min(5,Math.floor(e/100))),o=Math.min(12,t*.35);return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"space-between",height:"100%",padding:"0 8px"},children:Array.from({length:n},(r,i)=>(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:0,flex:1},children:[(0,_.jsx)("div",{style:{width:o,height:o,borderRadius:"50%",border:"1.5px solid var(--agd-stroke)",background:i===0?"var(--agd-bar)":"transparent",flexShrink:0}}),i<n-1&&(0,_.jsx)("div",{style:{flex:1,height:1,background:"var(--agd-stroke)",margin:"0 4px"}})]},i))})}function dw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:4,border:"1px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",justifyContent:"center",gap:4,padding:"0 6px"},children:[(0,_.jsx)(W,{w:Math.max(16,e*.5),h:2,strong:!0}),(0,_.jsx)("div",{style:{width:8,height:8,borderRadius:"50%",border:"1px solid var(--agd-stroke)",flexShrink:0}})]})}function _w({width:e,height:t}){let o=Math.min(t*.7,e/7.5);return(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:"100%",gap:o*.2},children:Array.from({length:5},(r,i)=>(0,_.jsx)("svg",{width:o,height:o,viewBox:"0 0 16 16",fill:"none",children:(0,_.jsx)("path",{d:"M8 1.5l2 4 4.5.7-3.25 3.1.75 4.5L8 11.4l-4 2.4.75-4.5L1.5 6.2 6 5.5z",stroke:"var(--agd-stroke)",strokeWidth:"0.8",fill:i<3?"var(--agd-bar)":"none"})},i))})}function mw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",position:"relative",borderRadius:4,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",overflow:"hidden"},children:[(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",style:{position:"absolute",inset:0},children:[(0,_.jsx)("line",{x1:0,y1:t*.3,x2:e,y2:t*.7,stroke:"var(--agd-stroke)",strokeWidth:"0.5",opacity:".2"}),(0,_.jsx)("line",{x1:0,y1:t*.6,x2:e,y2:t*.2,stroke:"var(--agd-stroke)",strokeWidth:"0.5",opacity:".15"}),(0,_.jsx)("line",{x1:e*.4,y1:0,x2:e*.6,y2:t,stroke:"var(--agd-stroke)",strokeWidth:"0.5",opacity:".15"})]}),(0,_.jsx)("div",{style:{position:"absolute",left:"50%",top:"40%",transform:"translate(-50%, -100%)"},children:(0,_.jsxs)("svg",{width:"16",height:"22",viewBox:"0 0 16 22",fill:"none",children:[(0,_.jsx)("path",{d:"M8 0C3.6 0 0 3.6 0 8c0 6 8 14 8 14s8-8 8-14c0-4.4-3.6-8-8-8z",fill:"var(--agd-bar)",opacity:".4"}),(0,_.jsx)("circle",{cx:"8",cy:"8",r:"3",fill:"var(--agd-fill)"})]})})]})}function pw({width:e,height:t}){let n=Math.max(3,Math.min(5,Math.floor(t/60)));return(0,_.jsxs)("div",{style:{display:"flex",height:"100%",padding:"8px 0"},children:[(0,_.jsx)("div",{style:{width:16,display:"flex",flexDirection:"column",alignItems:"center"},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",flex:1},children:[(0,_.jsx)(bn,{size:8}),r<n-1&&(0,_.jsx)("div",{style:{flex:1,width:1,background:"var(--agd-stroke)"}})]},r))}),(0,_.jsx)("div",{style:{flex:1,display:"flex",flexDirection:"column",justifyContent:"space-around",paddingLeft:8},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(W,{w:`${35+r*13%25}%`,h:3,strong:!0}),(0,_.jsx)(W,{w:`${50+r*17%30}%`,h:2})]},r))})]})}function fw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:8,border:"2px dashed var(--agd-stroke)",display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",gap:t*.06},children:[(0,_.jsxs)("svg",{width:"24",height:"24",viewBox:"0 0 24 24",fill:"none",children:[(0,_.jsx)("path",{d:"M12 16V4m0 0l-4 4m4-4l4 4",stroke:"var(--agd-stroke)",strokeWidth:"1.5"}),(0,_.jsx)("path",{d:"M4 17v2a1 1 0 001 1h14a1 1 0 001-1v-2",stroke:"var(--agd-stroke)",strokeWidth:"1.5"})]}),(0,_.jsx)(W,{w:e*.4,h:2}),(0,_.jsx)(W,{w:e*.25,h:2})]})}function gw({width:e,height:t}){let n=Math.max(3,Math.min(8,Math.floor(t/20)));return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:6,background:"var(--agd-fill)",border:"1px solid var(--agd-stroke)",padding:8,display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsxs)("div",{style:{display:"flex",gap:3,marginBottom:4},children:[(0,_.jsx)(bn,{size:6}),(0,_.jsx)(bn,{size:6}),(0,_.jsx)(bn,{size:6})]}),Array.from({length:n},(o,r)=>(0,_.jsx)("div",{style:{display:"flex",gap:6,paddingLeft:r>0&&r<n-1?12:0},children:(0,_.jsx)(W,{w:`${25+r*23%50}%`,h:2,strong:r===0})},r))]})}function hw({width:e,height:t}){let r=Math.min((e-16)/7,(t-40)/6);return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",justifyContent:"space-between",padding:"6px 8px"},children:[(0,_.jsx)("span",{style:{fontSize:8,color:"var(--agd-stroke)"},children:"\u2039"}),(0,_.jsx)(W,{w:e*.3,h:3,strong:!0}),(0,_.jsx)("span",{style:{fontSize:8,color:"var(--agd-stroke)"},children:"\u203A"})]}),(0,_.jsxs)("div",{style:{display:"grid",gridTemplateColumns:"repeat(7, 1fr)",gap:2,padding:"0 4px",flex:1},children:[Array.from({length:7},(i,s)=>(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:r*.6},children:(0,_.jsx)(W,{w:r*.5,h:2})},`h${s}`)),Array.from({length:35},(i,s)=>(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:r},children:(0,_.jsx)("div",{style:{width:r*.6,height:r*.6,borderRadius:"50%",background:s===12?"var(--agd-bar)":"transparent",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)("div",{style:{width:2,height:2,borderRadius:1,background:"var(--agd-bar-strong)",opacity:s===12?1:.3}})})},s))]})]})}function yw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",borderRadius:8,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:"0 10px",gap:8},children:[(0,_.jsx)(bn,{size:Math.min(32,t*.55)}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(W,{w:"50%",h:3,strong:!0}),(0,_.jsx)(W,{w:"75%",h:2})]}),(0,_.jsx)(W,{w:30,h:2})]})}function xw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column"},children:[(0,_.jsx)("div",{style:{height:"50%",background:"var(--agd-fill)",borderBottom:"1px dashed var(--agd-stroke)"}}),(0,_.jsxs)("div",{style:{flex:1,padding:10,display:"flex",flexDirection:"column",gap:5},children:[(0,_.jsx)(W,{w:"65%",h:4,strong:!0}),(0,_.jsx)(W,{w:"40%",h:3}),(0,_.jsx)("div",{style:{flex:1}}),(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",justifyContent:"space-between"},children:[(0,_.jsx)(W,{w:"30%",h:5,strong:!0}),(0,_.jsx)(pt,{w:Math.min(70,e*.3),h:26,radius:4,style:{background:"var(--agd-bar)"}})]})]})]})}function vw({width:e,height:t}){let n=Math.min(48,t*.3);return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",justifyContent:"center",gap:t*.06},children:[(0,_.jsx)(bn,{size:n}),(0,_.jsx)(W,{w:e*.45,h:4,strong:!0}),(0,_.jsx)(W,{w:e*.3,h:2}),(0,_.jsxs)("div",{style:{display:"flex",gap:e*.08,marginTop:t*.04},children:[(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",gap:2},children:[(0,_.jsx)(W,{w:20,h:3,strong:!0}),(0,_.jsx)(W,{w:28,h:2})]}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",gap:2},children:[(0,_.jsx)(W,{w:20,h:3,strong:!0}),(0,_.jsx)(W,{w:28,h:2})]}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",gap:2},children:[(0,_.jsx)(W,{w:20,h:3,strong:!0}),(0,_.jsx)(W,{w:28,h:2})]})]})]})}function ww({width:e,height:t}){let n=Math.max(e*.6,80),o=Math.max(3,Math.floor(t/40));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex"},children:[(0,_.jsx)("div",{style:{width:e-n,background:"var(--agd-fill)",opacity:.3}}),(0,_.jsxs)("div",{style:{flex:1,borderLeft:"1px solid var(--agd-stroke)",display:"flex",flexDirection:"column",padding:e*.04},children:[(0,_.jsxs)("div",{style:{display:"flex",justifyContent:"space-between",alignItems:"center",marginBottom:t*.06},children:[(0,_.jsx)(W,{w:n*.4,h:4,strong:!0}),(0,_.jsx)("div",{style:{width:12,height:12,border:"1px solid var(--agd-stroke)",borderRadius:3}})]}),Array.from({length:o},(r,i)=>(0,_.jsx)("div",{style:{padding:"6px 0"},children:(0,_.jsx)(W,{w:`${50+i*17%35}%`,h:2,strong:i===0})},i))]})]})}function bw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center"},children:[(0,_.jsxs)("div",{style:{flex:1,width:"100%",borderRadius:8,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",padding:10,display:"flex",flexDirection:"column",gap:5},children:[(0,_.jsx)(W,{w:"70%",h:3,strong:!0}),(0,_.jsx)(W,{w:"90%",h:2}),(0,_.jsx)(W,{w:"60%",h:2})]}),(0,_.jsx)("div",{style:{width:10,height:10,background:"var(--agd-fill)",border:"1px dashed var(--agd-stroke)",borderTop:"none",borderLeft:"none",transform:"rotate(45deg)",marginTop:-6}})]})}function kw({width:e,height:t}){let n=Math.min(t*.7,e*.3);return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",alignItems:"center",gap:e*.08},children:[(0,_.jsx)(pt,{w:n,h:n,radius:n*.25}),(0,_.jsx)(W,{w:e*.45,h:Math.max(4,t*.2),strong:!0})]})}function Sw({width:e,height:t}){let n=Math.max(2,Math.min(5,Math.floor(t/56)));return(0,_.jsx)("div",{style:{display:"flex",flexDirection:"column",height:"100%"},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{borderBottom:"1px solid var(--agd-stroke)",padding:"8px 6px",display:"flex",alignItems:"center",justifyContent:"space-between",flex:r===0?2:1},children:[(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",gap:6},children:[(0,_.jsx)("span",{style:{fontSize:9,fontWeight:700,color:"var(--agd-stroke)"},children:"Q"}),(0,_.jsx)(W,{w:e*(.3+r*13%25/100),h:3,strong:!0})]}),(0,_.jsx)("span",{style:{fontSize:8,color:"var(--agd-stroke)"},children:r===0?"\u25BC":"\u25B6"})]},r))})}function Cw({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/120))),o=Math.max(1,Math.min(3,Math.floor(t/120)));return(0,_.jsx)("div",{style:{display:"grid",gridTemplateColumns:`repeat(${n}, 1fr)`,gridTemplateRows:`repeat(${o}, 1fr)`,gap:4,height:"100%"},children:Array.from({length:n*o},(r,i)=>(0,_.jsx)("div",{style:{borderRadius:4,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",position:"relative",overflow:"hidden"},children:(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:"0 0 100 100",preserveAspectRatio:"none",fill:"none",children:[(0,_.jsx)("line",{x1:"0",y1:"0",x2:"100",y2:"100",stroke:"var(--agd-stroke)",strokeWidth:"0.5"}),(0,_.jsx)("line",{x1:"100",y1:"0",x2:"0",y2:"100",stroke:"var(--agd-stroke)",strokeWidth:"0.5"})]})},i))})}function Ew({width:e,height:t}){let n=Math.min(e,t);return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("rect",{x:"1",y:(t-n+2)/2,width:n-2,height:n-2,rx:n*.15,stroke:"var(--agd-stroke)",strokeWidth:"1.5"}),(0,_.jsx)("path",{d:`M${n*.25} ${t/2}l${n*.2} ${n*.2} ${n*.3}-${n*.35}`,stroke:"var(--agd-bar)",strokeWidth:"1.5",fill:"none",strokeLinecap:"round",strokeLinejoin:"round"})]})}function Mw({width:e,height:t}){let n=Math.min(e,t)/2-1;return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("circle",{cx:e/2,cy:t/2,r:n,stroke:"var(--agd-stroke)",strokeWidth:"1.5"}),(0,_.jsx)("circle",{cx:e/2,cy:t/2,r:n*.45,fill:"var(--agd-bar)"})]})}function Tw({width:e,height:t}){let n=Math.max(2,t*.12),o=Math.min(t*.35,10),r=e*.55;return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",alignItems:"center",position:"relative"},children:[(0,_.jsx)("div",{style:{width:"100%",height:n,borderRadius:n/2,background:"var(--agd-fill)",border:"1px solid var(--agd-stroke)",position:"relative"},children:(0,_.jsx)("div",{style:{width:r,height:"100%",borderRadius:n/2,background:"var(--agd-bar)"}})}),(0,_.jsx)("div",{style:{position:"absolute",left:r-o,width:o*2,height:o*2,borderRadius:"50%",border:"1.5px solid var(--agd-stroke)",background:"var(--agd-fill)"}})]})}function Lw({width:e,height:t}){let n=Math.min(36,t*.15),o=7,r=4,i=Math.min((e-16)/o,(t-n-40)/(r+1));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsxs)("div",{style:{height:n,borderRadius:4,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:"0 8px",justifyContent:"space-between"},children:[(0,_.jsx)(W,{w:"40%",h:2}),(0,_.jsxs)("svg",{width:"12",height:"12",viewBox:"0 0 16 16",fill:"none",children:[(0,_.jsx)("rect",{x:"2",y:"3",width:"12",height:"11",rx:"1",stroke:"var(--agd-stroke)",strokeWidth:"1"}),(0,_.jsx)("line",{x1:"2",y1:"6",x2:"14",y2:"6",stroke:"var(--agd-stroke)",strokeWidth:"0.5"})]})]}),(0,_.jsxs)("div",{style:{flex:1,borderRadius:6,border:"1px dashed var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",flexDirection:"column"},children:[(0,_.jsxs)("div",{style:{display:"flex",alignItems:"center",justifyContent:"space-between",padding:"4px 6px"},children:[(0,_.jsx)("span",{style:{fontSize:7,color:"var(--agd-stroke)"},children:"\u2039"}),(0,_.jsx)(W,{w:e*.25,h:2,strong:!0}),(0,_.jsx)("span",{style:{fontSize:7,color:"var(--agd-stroke)"},children:"\u203A"})]}),(0,_.jsx)("div",{style:{display:"grid",gridTemplateColumns:`repeat(${o}, 1fr)`,gap:1,padding:"0 4px",flex:1},children:Array.from({length:o*r},(s,l)=>(0,_.jsx)("div",{style:{display:"flex",alignItems:"center",justifyContent:"center",height:i},children:(0,_.jsx)("div",{style:{width:i*.5,height:i*.5,borderRadius:"50%",background:l===10?"var(--agd-bar)":"transparent"},children:(0,_.jsx)("div",{style:{width:"100%",height:"100%",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)("div",{style:{width:1.5,height:1.5,borderRadius:1,background:"var(--agd-bar-strong)",opacity:l===10?1:.25}})})})},l))})]})]})}function Rw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",gap:t*.08,padding:4},children:[(0,_.jsx)("div",{style:{width:"100%",height:t*.2,borderRadius:4,background:"var(--agd-fill)"}}),(0,_.jsx)("div",{style:{width:"70%",height:Math.max(6,t*.1),borderRadius:3,background:"var(--agd-fill)"}}),(0,_.jsx)("div",{style:{width:"90%",height:Math.max(4,t*.06),borderRadius:3,background:"var(--agd-fill)"}}),(0,_.jsx)("div",{style:{width:"50%",height:Math.max(4,t*.06),borderRadius:3,background:"var(--agd-fill)"}})]})}function Pw({width:e,height:t}){return(0,_.jsx)("div",{style:{height:"100%",display:"flex",alignItems:"center",gap:6},children:(0,_.jsxs)("div",{style:{height:"100%",flex:1,borderRadius:t/2,border:"1px solid var(--agd-stroke)",background:"var(--agd-fill)",display:"flex",alignItems:"center",padding:`0 ${t*.3}px`,gap:4},children:[(0,_.jsx)(W,{w:"60%",h:2,strong:!0}),(0,_.jsx)("div",{style:{width:Math.max(6,t*.3),height:Math.max(6,t*.3),borderRadius:"50%",border:"1px solid var(--agd-stroke)",flexShrink:0,marginLeft:"auto"}})]})})}function Nw({width:e,height:t}){let n=Math.min(e,t);return(0,_.jsx)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:(0,_.jsx)("path",{d:`M${e/2} ${(t-n)/2+n*.1}l${n*.12} ${n*.25} ${n*.28} ${n*.04}-${n*.2} ${n*.2} ${n*.05} ${n*.28}-${n*.25}-${n*.12}-${n*.25} ${n*.12} ${n*.05}-${n*.28}-${n*.2}-${n*.2} ${n*.28}-${n*.04}z`,stroke:"var(--agd-stroke)",strokeWidth:"1",fill:"var(--agd-fill)"})})}function Iw({width:e,height:t}){let n=Math.min(e,t)/2-2;return(0,_.jsxs)("svg",{width:"100%",height:"100%",viewBox:`0 0 ${e} ${t}`,fill:"none",children:[(0,_.jsx)("circle",{cx:e/2,cy:t/2,r:n,stroke:"var(--agd-stroke)",strokeWidth:"1.5",opacity:".2"}),(0,_.jsx)("path",{d:`M${e/2} ${t/2-n}a${n} ${n} 0 0 1 ${n} ${n}`,stroke:"var(--agd-bar-strong)",strokeWidth:"1.5",strokeLinecap:"round"})]})}function Dw({width:e,height:t}){let n=Math.min(36,t*.25,e*.12),o=Math.max(1,Math.min(3,Math.floor(t/80)));return(0,_.jsx)("div",{style:{display:"flex",flexDirection:"column",height:"100%",justifyContent:"space-around",padding:8},children:Array.from({length:o},(r,i)=>(0,_.jsxs)("div",{style:{display:"flex",gap:e*.04,alignItems:"flex-start"},children:[(0,_.jsx)(pt,{w:n,h:n,radius:n*.25}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:4},children:[(0,_.jsx)(W,{w:`${40+i*13%20}%`,h:3,strong:!0}),(0,_.jsx)(W,{w:`${60+i*17%25}%`,h:2})]})]},i))})}function Aw({width:e,height:t}){let n=Math.max(2,Math.min(4,Math.floor(e/120))),o=Math.min(36,t*.25);return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",gap:t*.06,padding:t*.06},children:[(0,_.jsx)(W,{w:e*.3,h:4,strong:!0}),(0,_.jsx)("div",{style:{display:"flex",gap:e*.06,justifyContent:"center",flex:1,alignItems:"center"},children:Array.from({length:n},(r,i)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",alignItems:"center",gap:6},children:[(0,_.jsx)(bn,{size:o}),(0,_.jsx)(W,{w:e*.12,h:3,strong:!0}),(0,_.jsx)(W,{w:e*.08,h:2})]},i))})]})}function $w({width:e,height:t}){let n=Math.max(2,Math.min(3,Math.floor(t/80)));return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",alignItems:"center",padding:e*.06,gap:t*.04},children:[(0,_.jsx)(W,{w:e*.5,h:Math.max(5,t*.04),strong:!0}),(0,_.jsx)(W,{w:e*.35,h:2}),(0,_.jsx)("div",{style:{width:"100%",display:"flex",flexDirection:"column",gap:t*.03,marginTop:t*.04},children:Array.from({length:n},(o,r)=>(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(W,{w:Math.min(60,e*.2),h:2}),(0,_.jsx)(pt,{w:"100%",h:Math.min(32,t*.1),radius:4})]},r))}),(0,_.jsx)(pt,{w:"100%",h:Math.min(36,t*.12),radius:6,style:{marginTop:t*.03,background:"var(--agd-bar)"}}),(0,_.jsx)(W,{w:e*.4,h:2})]})}function Fw({width:e,height:t}){return(0,_.jsxs)("div",{style:{height:"100%",display:"flex",flexDirection:"column",padding:e*.04,gap:t*.03},children:[(0,_.jsx)(W,{w:e*.4,h:4,strong:!0}),(0,_.jsx)(W,{w:e*.6,h:2}),(0,_.jsxs)("div",{style:{display:"flex",gap:6,marginTop:t*.03},children:[(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(W,{w:50,h:2}),(0,_.jsx)(pt,{w:"100%",h:Math.min(28,t*.1),radius:4})]}),(0,_.jsxs)("div",{style:{flex:1,display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(W,{w:40,h:2}),(0,_.jsx)(pt,{w:"100%",h:Math.min(28,t*.1),radius:4})]})]}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:3},children:[(0,_.jsx)(W,{w:50,h:2}),(0,_.jsx)(pt,{w:"100%",h:Math.min(28,t*.1),radius:4})]}),(0,_.jsxs)("div",{style:{display:"flex",flexDirection:"column",gap:3,flex:1},children:[(0,_.jsx)(W,{w:60,h:2}),(0,_.jsx)(pt,{w:"100%",h:"100%",radius:4})]}),(0,_.jsx)(pt,{w:Math.min(120,e*.3),h:Math.min(30,t*.1),radius:6,style:{alignSelf:"flex-end",background:"var(--agd-bar)"}})]})}var Ow={navigation:Ev,hero:Mv,sidebar:Tv,footer:Lv,modal:Rv,card:Pv,text:Nv,image:Iv,table:Dv,list:Av,button:$v,input:Fv,form:Ov,tabs:Bv,avatar:zv,badge:Hv,header:Wv,section:Uv,grid:jv,dropdown:Yv,toggle:Vv,search:Xv,toast:Qv,progress:Gv,chart:qv,video:Kv,tooltip:Jv,breadcrumb:Zv,pagination:ew,divider:tw,accordion:nw,carousel:ow,pricing:rw,testimonial:iw,cta:sw,alert:lw,banner:aw,stat:cw,stepper:uw,tag:dw,rating:_w,map:mw,timeline:pw,fileUpload:fw,codeBlock:gw,calendar:hw,notification:yw,productCard:xw,profile:vw,drawer:ww,popover:bw,logo:kw,faq:Sw,gallery:Cw,checkbox:Ew,radio:Mw,slider:Tw,datePicker:Lw,skeleton:Rw,chip:Pw,icon:Nw,spinner:Iw,feature:Dw,team:Aw,login:$w,contact:Fw};function My({type:e,width:t,height:n,text:o}){let r=Ow[e];return r?(0,_.jsx)("div",{style:{width:"100%",height:"100%",padding:8,position:"relative",pointerEvents:"none"},children:(0,_.jsx)(r,{width:t,height:n,text:o})}):(0,_.jsx)("div",{style:{width:"100%",height:"100%",display:"flex",alignItems:"center",justifyContent:"center"},children:(0,_.jsx)("span",{style:{fontSize:10,fontWeight:600,color:"var(--agd-text-3)",textTransform:"uppercase",letterSpacing:"0.06em",opacity:.5},children:e})})}var Ty=`.styles-module__overlay___aWh-q svg[fill=none],
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
.styles-module__palette___C7iSH.styles-module__paletteTopRight___5oHU6 {
  top: 0;
  right: 0;
  bottom: auto;
  max-height: calc(100vh - 24px);
  overflow-y: auto;
  pointer-events: auto;
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
}`,$={overlay:"styles-module__overlay___aWh-q",rearrangeOverlay:"styles-module__rearrangeOverlay___-3R3t",overlayExiting:"styles-module__overlayExiting___iEmYr",overlayFadeIn:"styles-module__overlayFadeIn___aECVy",light:"styles-module__light___ORIft",wireframe:"styles-module__wireframe___itvQU",placing:"styles-module__placing___45yD8",passthrough:"styles-module__passthrough___xaFeE",blankCanvas:"styles-module__blankCanvas___t2Eue",visible:"styles-module__visible___OKKqX",gridActive:"styles-module__gridActive___OZ-cf",paletteHeader:"styles-module__paletteHeader___-Q5gQ",paletteHeaderTitle:"styles-module__paletteHeaderTitle___oHqZC",paletteHeaderDesc:"styles-module__paletteHeaderDesc___6i74T",wireframePurposeWrap:"styles-module__wireframePurposeWrap___To-tS",collapsed:"styles-module__collapsed___Ms9vS",wireframePurposeInner:"styles-module__wireframePurposeInner___Lrahs",wireframePurposeInput:"styles-module__wireframePurposeInput___7EtBN",canvasToggle:"styles-module__canvasToggle___-QqSy",active:"styles-module__active___hosp7",canvasToggleIcon:"styles-module__canvasToggleIcon___7pJ82",canvasToggleLabel:"styles-module__canvasToggleLabel___OanpY",placement:"styles-module__placement___zcxv8",placementEnter:"styles-module__placementEnter___TdRhf",selected:"styles-module__selected___6yrp6",dragging:"styles-module__dragging___le6KZ",exiting:"styles-module__exiting___YrM8F",placementContent:"styles-module__placementContent___f64A4",placementLabel:"styles-module__placementLabel___0KvWl",placementAnnotation:"styles-module__placementAnnotation___78pTr",annotationVisible:"styles-module__annotationVisible___mrUyA",sectionAnnotation:"styles-module__sectionAnnotation___aUIs0",handle:"styles-module__handle___Ikbxm",sectionOutline:"styles-module__sectionOutline___s0hy-",ghostOutline:"styles-module__ghostOutline___po-kO",handleNw:"styles-module__handleNw___4TMIj",handleNe:"styles-module__handleNe___mnsTh",handleSe:"styles-module__handleSe___oSFnk",handleSw:"styles-module__handleSw___pi--Z",handleN:"styles-module__handleN___aBA-Q",handleE:"styles-module__handleE___0hM5u",handleS:"styles-module__handleS___JjDRv",handleW:"styles-module__handleW___ERWGQ",edgeHandle:"styles-module__edgeHandle___XxXdT",edgeN:"styles-module__edgeN___-JJDj",edgeS:"styles-module__edgeS___66lMX",edgeE:"styles-module__edgeE___1bGDa",edgeW:"styles-module__edgeW___lHQNo",deleteButton:"styles-module__deleteButton___LkGCb",drawBox:"styles-module__drawBox___BrVAa",selectBox:"styles-module__selectBox___Iu8kB",sizeIndicator:"styles-module__sizeIndicator___7zJ4y",guideLine:"styles-module__guideLine___DUQY2",dragPreview:"styles-module__dragPreview___onPbU",dragPreviewWireframe:"styles-module__dragPreviewWireframe___jsg0G",palette:"styles-module__palette___C7iSH",paletteItem:"styles-module__paletteItem___6TlnA",paletteItemLabel:"styles-module__paletteItemLabel___6ncO4",paletteSectionTitle:"styles-module__paletteSectionTitle___PqnjX",paletteFooter:"styles-module__paletteFooter___QYnAG",paletteTopRight:"styles-module__paletteTopRight___5oHU6",paletteSection:"styles-module__paletteSection___V8DEA",paletteItemIcon:"styles-module__paletteItemIcon___0NPQK",placeScroll:"styles-module__placeScroll___7sClM",fadeTop:"styles-module__fadeTop___KT9tF",fadeBottom:"styles-module__fadeBottom___x3ShT",paletteFooterWrap:"styles-module__paletteFooterWrap___71-fI",footerHidden:"styles-module__footerHidden___fJUik",paletteFooterInnerContent:"styles-module__paletteFooterInnerContent___VC26h",paletteFooterInner:"styles-module__paletteFooterInner___dfylY",paletteFooterCount:"styles-module__paletteFooterCount___D3Fia",paletteFooterClear:"styles-module__paletteFooterClear___ybBoa",paletteFooterActions:"styles-module__paletteFooterActions___fLzv8",rollingWrap:"styles-module__rollingWrap___S75jM",rollingNum:"styles-module__rollingNum___1RKDx",exitUp:"styles-module__exitUp___AFDRW",numExitUp:"styles-module__numExitUp___FRQqx",enterUp:"styles-module__enterUp___CPlXb",numEnterUp:"styles-module__numEnterUp___2Yd-w",exitDown:"styles-module__exitDown___-1yAy",numExitDown:"styles-module__numExitDown___xm5by",enterDown:"styles-module__enterDown___DDuFR",numEnterDown:"styles-module__numEnterDown___hpxBk",hoverHighlight:"styles-module__hoverHighlight___8eT-v",highlightFadeIn:"styles-module__highlightFadeIn___Lg7KY",sectionEnter:"styles-module__sectionEnter___-8BXT",settled:"styles-module__settled___b5U5o",sectionLabel:"styles-module__sectionLabel___F80HQ",movedBadge:"styles-module__movedBadge___s8z-q",sectionDimensions:"styles-module__sectionDimensions___RcJSL",badgeVisible:"styles-module__badgeVisible___npbdS",resizedBadge:"styles-module__resizedBadge___u51V8",wireframeNotice:"styles-module__wireframeNotice___4GJyB",wireframeOpacityRow:"styles-module__wireframeOpacityRow___CJXzi",wireframeOpacityLabel:"styles-module__wireframeOpacityLabel___afkfT",wireframeOpacitySlider:"styles-module__wireframeOpacitySlider___YcoEs",wireframeNoticeTitleRow:"styles-module__wireframeNoticeTitleRow___PJqyG",wireframeNoticeTitle:"styles-module__wireframeNoticeTitle___okr08",wireframeNoticeDivider:"styles-module__wireframeNoticeDivider___PNKQ6",wireframeStartOver:"styles-module__wireframeStartOver___YFk-I",ghostEnter:"styles-module__ghostEnter___EC3Mb",ghostBadge:"styles-module__ghostBadge___tsQUK",badgeSlideIn:"styles-module__badgeSlideIn___typJ7",ghostBadgeExtra:"styles-module__ghostBadgeExtra___6CVoD",badgeExtraIn:"styles-module__badgeExtraIn___i4W8F",originalOutline:"styles-module__originalOutline___Y6DD1",originalLabel:"styles-module__originalLabel___HqI9g",connectorSvg:"styles-module__connectorSvg___Lovld",connectorLine:"styles-module__connectorLine___XeWh-",connectorDraw:"styles-module__connectorDraw___8sK5I",connectorDot:"styles-module__connectorDot___yvf7C",connectorDotIn:"styles-module__connectorDotIn___NwTUq",connectorExiting:"styles-module__connectorExiting___2lLOs",connectorOut:"styles-module__connectorOut___5QoPl",connectorDotOut:"styles-module__connectorDotOut___FEq7e"};var xt=ae(Be()),ds=24,tu=5;function Ly(e,t,n,o,r){let i=1/0,s=1/0,l=e.x,a=e.x+e.width,c=e.x+e.width/2,d=e.y,m=e.y+e.height,f=e.y+e.height/2,k=!o,w=k?[l,a,c]:[...o.left?[l]:[],...o.right?[a]:[]],C=k?[d,m,f]:[...o.top?[d]:[],...o.bottom?[m]:[]],S=[];for(let Z of t)n.has(Z.id)||S.push(Z);r&&S.push(...r);for(let Z of S){let Ie=Z.x,ye=Z.x+Z.width,me=Z.x+Z.width/2,Me=Z.y,B=Z.y+Z.height,dt=Z.y+Z.height/2;for(let ut of w)for(let ue of[Ie,ye,me]){let $e=ue-ut;Math.abs($e)<tu&&Math.abs($e)<Math.abs(i)&&(i=$e)}for(let ut of C)for(let ue of[Me,B,dt]){let $e=ue-ut;Math.abs($e)<tu&&Math.abs($e)<Math.abs(s)&&(s=$e)}}let g=Math.abs(i)<tu?i:0,x=Math.abs(s)<tu?s:0,h=[],E=new Set,O=l+g,j=a+g,A=c+g,Y=d+x,K=m+x,q=f+x;for(let Z of S){let Ie=Z.x,ye=Z.x+Z.width,me=Z.x+Z.width/2,Me=Z.y,B=Z.y+Z.height,dt=Z.y+Z.height/2;for(let ut of[Ie,me,ye])for(let ue of[O,A,j])if(Math.abs(ue-ut)<.5){let $e=`x:${Math.round(ut)}`;E.has($e)||(E.add($e),h.push({axis:"x",pos:ut}))}for(let ut of[Me,dt,B])for(let ue of[Y,q,K])if(Math.abs(ue-ut)<.5){let $e=`y:${Math.round(ut)}`;E.has($e)||(E.add($e),h.push({axis:"y",pos:ut}))}}return{dx:g,dy:x,guides:h}}function Ry(){return`dp-${Date.now()}-${Math.random().toString(36).slice(2,7)}`}function Py({placements:e,onChange:t,activeComponent:n,onActiveComponentChange:o,isDarkMode:r,exiting:i,onInteractionChange:s,className:l,passthrough:a,extraSnapRects:c,onSelectionChange:d,deselectSignal:m,onDragMove:f,onDragEnd:k,clearingPlacements:w,wireframe:C}){let[S,g]=(0,lt.useState)(new Set),[x,h]=(0,lt.useState)(null),[E,O]=(0,lt.useState)(null),[j,A]=(0,lt.useState)(null),[Y,K]=(0,lt.useState)([]),[q,Z]=(0,lt.useState)(null),[Ie,ye]=(0,lt.useState)(!1),me=(0,lt.useRef)(!1),[Me,B]=(0,lt.useState)(new Set),dt=(0,lt.useRef)(new Map),ut=(0,lt.useRef)(null),ue=(0,lt.useRef)(null),$e=(0,lt.useRef)(e);$e.current=e;let Fe=(0,lt.useRef)(d);Fe.current=d;let vt=(0,lt.useRef)(f);vt.current=f;let Ct=(0,lt.useRef)(k);Ct.current=k;let Ht=(0,lt.useRef)(m);(0,lt.useEffect)(()=>{m!==Ht.current&&(Ht.current=m,g(new Set))},[m]),(0,lt.useEffect)(()=>{w?.length&&(g(G=>new Set([...G].filter(ie=>!w.some(xe=>xe.id===ie)))),ue.current=null)},[w]),(0,lt.useEffect)(()=>{let G=ie=>{let xe=ie.composedPath()[0]||ie.target;if(!(xe.tagName==="INPUT"||xe.tagName==="TEXTAREA"||xe.isContentEditable)){if((ie.key==="Backspace"||ie.key==="Delete")&&S.size>0){ie.preventDefault();let Se=new Set(S);B(Se),g(new Set),Ne(()=>{t($e.current.filter(gt=>!Se.has(gt.id))),B(new Set)},180);return}if(["ArrowUp","ArrowDown","ArrowLeft","ArrowRight"].includes(ie.key)&&S.size>0){ie.preventDefault();let Se=ie.shiftKey?20:1,gt=ie.key==="ArrowLeft"?-Se:ie.key==="ArrowRight"?Se:0,rt=ie.key==="ArrowUp"?-Se:ie.key==="ArrowDown"?Se:0;t(e.map(Ge=>S.has(Ge.id)?{...Ge,x:Math.max(0,Ge.x+gt),y:Math.max(0,Ge.y+rt)}:Ge));return}if(ie.key==="Escape"){n?o(null):S.size>0&&g(new Set);return}}};return document.addEventListener("keydown",G),()=>document.removeEventListener("keydown",G)},[S,n,e,t,o]);let Zt=(0,lt.useCallback)(G=>{if(G.button!==0||a||G.target.closest(`.${$.placement}`))return;G.preventDefault(),G.stopPropagation();let xe=window.scrollY,De=G.clientX,Se=G.clientY;if(n){ue.current="place",s?.(!0);let gt=!1,rt=De,Ge=Se,Ye=L=>{rt=L.clientX,Ge=L.clientY;let D=Math.abs(rt-De),z=Math.abs(Ge-Se);if((D>5||z>5)&&(gt=!0),gt){let U=Math.min(De,rt),se=Math.min(Se,Ge),ce=Math.abs(rt-De),R=Math.abs(Ge-Se);h({x:U,y:se,w:ce,h:R}),A({x:L.clientX+12,y:L.clientY+12,text:`${Math.round(ce)} \xD7 ${Math.round(R)}`})}},Lt=L=>{window.removeEventListener("mousemove",Ye),window.removeEventListener("mouseup",Lt),h(null),A(null),ue.current=null,s?.(!1);let D=ee[n],z,U,se,ce;gt?(z=Math.min(De,rt),U=Math.min(Se,Ge)+xe,se=Math.max(ds,Math.abs(rt-De)),ce=Math.max(ds,Math.abs(Ge-Se))):(se=D.width,ce=D.height,z=De-se/2,U=Se+xe-ce/2),z=Math.max(0,z),U=Math.max(0,U);let R={id:Ry(),type:n,x:z,y:U,width:se,height:ce,scrollY:xe,timestamp:Date.now()},le=[...e,R];t(le),g(new Set([R.id])),o(null)};window.addEventListener("mousemove",Ye),window.addEventListener("mouseup",Lt)}else{G.shiftKey||g(new Set),ue.current="select";let gt=!1,rt=Ye=>{let Lt=Math.abs(Ye.clientX-De),L=Math.abs(Ye.clientY-Se);if((Lt>4||L>4)&&(gt=!0),gt){let D=Math.min(De,Ye.clientX),z=Math.min(Se,Ye.clientY);O({x:D,y:z,w:Math.abs(Ye.clientX-De),h:Math.abs(Ye.clientY-Se)})}},Ge=Ye=>{if(window.removeEventListener("mousemove",rt),window.removeEventListener("mouseup",Ge),ue.current=null,gt){let Lt=Math.min(De,Ye.clientX),L=Math.min(Se,Ye.clientY)+xe,D=Math.abs(Ye.clientX-De),z=Math.abs(Ye.clientY-Se),U=new Set(G.shiftKey?S:new Set);for(let se of e){let ce=se.y-xe;se.x+se.width>Lt&&se.x<Lt+D&&se.y+se.height>L&&se.y<L+z&&U.add(se.id)}g(U)}O(null)};window.addEventListener("mousemove",rt),window.addEventListener("mouseup",Ge)}},[n,a,e,t,S]),ci=(0,lt.useCallback)((G,ie)=>{if(G.button!==0)return;let xe=G.target;if(xe.closest(`.${$.handle}`)||xe.closest(`.${$.deleteButton}`))return;G.preventDefault(),G.stopPropagation();let De;G.shiftKey?(De=new Set(S),De.has(ie)?De.delete(ie):De.add(ie)):S.has(ie)?De=new Set(S):De=new Set([ie]),g(De),(De.size!==S.size||[...De].some(le=>!S.has(le)))&&Fe.current?.(De,G.shiftKey);let gt=window.scrollY,rt=G.clientX,Ge=G.clientY,Ye=new Map;for(let le of e)De.has(le.id)&&Ye.set(le.id,{x:le.x,y:le.y});ue.current="move",s?.(!0);let Lt=!1,L=!1,D=e,z=0,U=0,se=new Map;for(let le of e)Ye.has(le.id)&&se.set(le.id,{w:le.width,h:le.height});let ce=le=>{let we=le.clientX-rt,He=le.clientY-Ge;if((Math.abs(we)>2||Math.abs(He)>2)&&(Lt=!0),!Lt)return;if(le.altKey&&!L){L=!0;let et=[];for(let ft of e)Ye.has(ft.id)&&et.push({...ft,id:Ry(),timestamp:Date.now()});D=[...e,...et]}let Je=1/0,it=1/0,We=-1/0,Ze=-1/0;for(let[et,ft]of Ye){let Wt=se.get(et);Wt&&(Je=Math.min(Je,ft.x+we),it=Math.min(it,ft.y+He),We=Math.max(We,ft.x+we+Wt.w),Ze=Math.max(Ze,ft.y+He+Wt.h))}let Te={x:Je,y:it,width:We-Je,height:Ze-it},{dx:wt,dy:ht,guides:at}=Ly(Te,D,new Set(Ye.keys()),void 0,c);K(at);let Ae=we+wt,Ve=He+ht;z=Ae,U=Ve,t(D.map(et=>{let ft=Ye.get(et.id);return ft?{...et,x:Math.max(0,ft.x+Ae),y:Math.max(0,ft.y+Ve)}:et})),vt.current?.(Ae,Ve)},R=()=>{window.removeEventListener("mousemove",ce),window.removeEventListener("mouseup",R),ue.current=null,s?.(!1),K([]),Ct.current?.(z,U,Lt)};window.addEventListener("mousemove",ce),window.addEventListener("mouseup",R)},[S,e,t,s]),mo=(0,lt.useCallback)((G,ie,xe)=>{G.preventDefault(),G.stopPropagation();let De=e.find(U=>U.id===ie);if(!De)return;g(new Set([ie])),ue.current="resize",s?.(!0);let Se=G.clientX,gt=G.clientY,rt=De.width,Ge=De.height,Ye=De.x,Lt=De.y,L={left:xe.includes("w"),right:xe.includes("e"),top:xe.includes("n"),bottom:xe.includes("s")},D=U=>{let se=U.clientX-Se,ce=U.clientY-gt,R=rt,le=Ge,we=Ye,He=Lt;xe.includes("e")&&(R=Math.max(ds,rt+se)),xe.includes("w")&&(R=Math.max(ds,rt-se),we=Ye+rt-R),xe.includes("s")&&(le=Math.max(ds,Ge+ce)),xe.includes("n")&&(le=Math.max(ds,Ge-ce),He=Lt+Ge-le);let Je={x:we,y:He,width:R,height:le},{dx:it,dy:We,guides:Ze}=Ly(Je,$e.current,new Set([ie]),L,c);K(Ze),it!==0&&(L.right?R+=it:L.left&&(we+=it,R-=it)),We!==0&&(L.bottom?le+=We:L.top&&(He+=We,le-=We)),t($e.current.map(Te=>Te.id===ie?{...Te,x:we,y:He,width:R,height:le}:Te)),A({x:U.clientX+12,y:U.clientY+12,text:`${Math.round(R)} \xD7 ${Math.round(le)}`})},z=()=>{window.removeEventListener("mousemove",D),window.removeEventListener("mouseup",z),A(null),ue.current=null,s?.(!1),K([])};window.addEventListener("mousemove",D),window.addEventListener("mouseup",z)},[e,t,s]),qo=(0,lt.useCallback)(G=>{ue.current=null,B(ie=>{let xe=new Set(ie);return xe.add(G),xe}),g(ie=>{let xe=new Set(ie);return xe.delete(G),xe}),Ne(()=>{t($e.current.filter(ie=>ie.id!==G)),B(ie=>{let xe=new Set(ie);return xe.delete(G),xe})},180)},[t]),po=new Set(["text","hero","button","badge","cta","toast","modal","card","navigation","tabs","input","search","breadcrumb","pricing","testimonial","alert","banner","tag","notification","stat","productCard"]),fo={hero:"Headline text",button:"Button label",badge:"Badge label",cta:"Call to action text",toast:"Notification message",modal:"Dialog title",card:"Card title",navigation:"Brand / nav items",tabs:"Tab labels",input:"Placeholder text",search:"Search placeholder",pricing:"Plan name or price",testimonial:"Quote text",alert:"Alert message",banner:"Banner text",tag:"Tag label",notification:"Notification message",stat:"Metric value",productCard:"Product name"},ze=(0,lt.useCallback)(G=>{let ie=e.find(xe=>xe.id===G);ie&&(me.current=!!ie.text,Z(G),ye(!1))},[e]),en=(0,lt.useCallback)(()=>{q&&(ye(!0),Ne(()=>{Z(null),ye(!1)},150))},[q]);(0,lt.useEffect)(()=>{i&&q&&en()},[i]);let go=(0,lt.useCallback)(G=>{q&&(t(e.map(ie=>ie.id===q?{...ie,text:G.trim()||void 0}:ie)),en())},[q,e,t,en]),Zn=typeof window<"u"?window.scrollY:0,Lr=["nw","ne","se","sw"],No=C?"#f97316":"#3c82f7",Ko=[{dir:"n",cls:$.edgeN,arrow:(0,xt.jsx)("svg",{width:"8",height:"6",viewBox:"0 0 8 6",fill:"none",children:(0,xt.jsx)("path",{d:"M4 0.5L1 4.5h6z",fill:No})})},{dir:"e",cls:$.edgeE,arrow:(0,xt.jsx)("svg",{width:"6",height:"8",viewBox:"0 0 6 8",fill:"none",children:(0,xt.jsx)("path",{d:"M5.5 4L1.5 1v6z",fill:No})})},{dir:"s",cls:$.edgeS,arrow:(0,xt.jsx)("svg",{width:"8",height:"6",viewBox:"0 0 8 6",fill:"none",children:(0,xt.jsx)("path",{d:"M4 5.5L1 1.5h6z",fill:No})})},{dir:"w",cls:$.edgeW,arrow:(0,xt.jsx)("svg",{width:"6",height:"8",viewBox:"0 0 6 8",fill:"none",children:(0,xt.jsx)("path",{d:"M0.5 4L4.5 1v6z",fill:No})})}];return(0,xt.jsxs)(xt.Fragment,{children:[(0,xt.jsx)("div",{ref:ut,className:`${$.overlay} ${r?"":$.light} ${n?$.placing:""} ${a?$.passthrough:""} ${i?$.overlayExiting:""} ${C?$.wireframe:""}${l?` ${l}`:""}`,"data-feedback-toolbar":!0,onMouseDown:Zt,children:e.map(G=>{let ie=S.has(G.id),xe=Hn[G.type]?.label||G.type,De=G.y-Zn;return(0,xt.jsxs)("div",{"data-design-placement":G.id,className:`${$.placement} ${ie?$.selected:""} ${Me.has(G.id)||w?.includes(G)?$.exiting:""}`,style:{left:G.x,top:De,width:G.width,height:G.height,position:"fixed"},onMouseDown:Se=>ci(Se,G.id),onDoubleClick:()=>ze(G.id),children:[(0,xt.jsx)("span",{className:$.placementLabel,children:xe}),(0,xt.jsx)("span",{className:`${$.placementAnnotation} ${G.text?$.annotationVisible:""}`,children:(G.text&&dt.current.set(G.id,G.text),G.text||dt.current.get(G.id)||"")}),(0,xt.jsx)("div",{className:$.placementContent,children:(0,xt.jsx)(My,{type:G.type,width:G.width,height:G.height,text:G.text})}),(0,xt.jsx)("div",{className:$.deleteButton,onMouseDown:Se=>Se.stopPropagation(),onClick:()=>qo(G.id),children:"\u2715"}),Lr.map(Se=>(0,xt.jsx)("div",{className:`${$.handle} ${$[`handle${Se.charAt(0).toUpperCase()}${Se.slice(1)}`]}`,onMouseDown:gt=>mo(gt,G.id,Se)},Se)),Ko.map(({dir:Se,cls:gt,arrow:rt})=>(0,xt.jsx)("div",{className:`${$.edgeHandle} ${gt}`,onMouseDown:Ge=>mo(Ge,G.id,Se),children:rt},Se))]},G.id)})}),q&&(()=>{let G=e.find(Lt=>Lt.id===q);if(!G)return null;let ie=G.y-Zn,xe=G.x+G.width/2,De=ie-8,Se=ie+G.height+8,gt=De>200,rt=Se<window.innerHeight-100,Ge=Math.max(160,Math.min(window.innerWidth-160,xe)),Ye;return gt?Ye={left:Ge,bottom:window.innerHeight-De}:rt?Ye={left:Ge,top:Se}:Ye={left:Ge,top:Math.max(80,window.innerHeight/2-80)},(0,xt.jsx)(cs,{element:Hn[G.type]?.label||G.type,placeholder:fo[G.type]||"Label or content text",initialValue:G.text??"",submitLabel:me.current?"Save":"Set",onSubmit:go,onCancel:en,onDelete:me.current?()=>{go("")}:void 0,isExiting:Ie,lightMode:!r,style:Ye})})(),x&&(0,xt.jsx)("div",{className:$.drawBox,style:{left:x.x,top:x.y,width:x.w,height:x.h},"data-feedback-toolbar":!0}),E&&(0,xt.jsx)("div",{className:$.selectBox,style:{left:E.x,top:E.y,width:E.w,height:E.h},"data-feedback-toolbar":!0}),j&&(0,xt.jsx)("div",{className:$.sizeIndicator,style:{left:j.x,top:j.y},"data-feedback-toolbar":!0,children:j.text}),Y.map((G,ie)=>(0,xt.jsx)("div",{className:$.guideLine,style:G.axis==="x"?{position:"fixed",left:G.pos,top:0,width:1,bottom:0}:{position:"fixed",left:0,top:G.pos-Zn,right:0,height:1},"data-feedback-toolbar":!0},`${G.axis}-${G.pos}-${ie}`))]})}var Kt=ae(bt());var Sr=ae(bt());function nu(e,{keepMounted:t=!1,onExited:n}={}){let[o,r]=(0,Sr.useState)(t||e),i=(0,Sr.useRef)(null),s=(0,Sr.useRef)(n);return e&&!o&&r(!0),(0,Sr.useLayoutEffect)(()=>{s.current=n},[n]),(0,Sr.useLayoutEffect)(()=>{let l=i.current;if(!l||l.dataset.panelOpen==="true"===e)return;getComputedStyle(l).opacity,l.dataset.panelPresent="true",l.dataset.panelOpen=String(e);let a=!1,c=l.getAnimations?.()??[];return Promise.allSettled(c.map(d=>d.finished)).then(()=>{a||e||(delete l.dataset.panelPresent,t||r(!1),s.current?.())}),()=>{a=!0}},[e,o,t]),{ref:i,mounted:o}}var u=ae(Be());function Bw(e){if(!e)return"";let t=e.scrollTop>2,n=e.scrollTop+e.clientHeight<e.scrollHeight-2;return`${t?$.fadeTop:""} ${n?$.fadeBottom:""}`}var v="currentColor",F="0.5";function zw({type:e}){switch(e){case"navigation":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"4",width:"18",height:"8",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"2.5",y:"7",width:"3",height:"1.5",rx:".5",fill:v,opacity:".4"}),(0,u.jsx)("rect",{x:"7",y:"7",width:"2.5",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"11",y:"7",width:"2.5",height:"1.5",rx:".5",fill:v,opacity:".25"})]});case"header":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"2",width:"18",height:"12",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"3",y:"5.5",width:"8",height:"2",rx:".5",fill:v,opacity:".35"}),(0,u.jsx)("rect",{x:"3",y:"9",width:"12",height:"1",rx:".5",fill:v,opacity:".15"})]});case"hero":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"1",width:"18",height:"14",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"5",y:"5",width:"10",height:"1.5",rx:".5",fill:v,opacity:".35"}),(0,u.jsx)("rect",{x:"7",y:"8",width:"6",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"7.5",y:"10.5",width:"5",height:"2.5",rx:"1",stroke:v,strokeWidth:F})]});case"section":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"1",width:"18",height:"14",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"3",y:"4",width:"6",height:"1",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"3",y:"6.5",width:"14",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"3",y:"9",width:"10",height:"1",rx:".5",fill:v,opacity:".15"})]});case"sidebar":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"1",width:"7",height:"14",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"2.5",y:"4",width:"4",height:"1",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"2.5",y:"6.5",width:"3.5",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"2.5",y:"9",width:"4",height:"1",rx:".5",fill:v,opacity:".15"})]});case"footer":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"7",width:"18",height:"8",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"3",y:"9.5",width:"4",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"9.5",width:"4",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"15",y:"9.5",width:"3",height:"1",rx:".5",fill:v,opacity:".2"})]});case"modal":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"12",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"5",y:"4.5",width:"7",height:"1",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"5",y:"7",width:"10",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"11",y:"11",width:"5",height:"2",rx:".75",stroke:v,strokeWidth:F})]});case"divider":return(0,u.jsx)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:(0,u.jsx)("line",{x1:"2",y1:"8",x2:"18",y2:"8",stroke:v,strokeWidth:"0.5",opacity:".3"})});case"card":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"14",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"5.5",rx:"1",fill:v,opacity:".04"}),(0,u.jsx)("rect",{x:"4",y:"8.5",width:"8",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"4",y:"11",width:"11",height:"1",rx:".5",fill:v,opacity:".12"})]});case"text":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4",width:"14",height:"1.5",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"2",y:"7",width:"11",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"2",y:"9.5",width:"13",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"2",y:"12",width:"8",height:"1",rx:".5",fill:v,opacity:".12"})]});case"image":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("line",{x1:"2",y1:"2",x2:"18",y2:"14",stroke:v,strokeWidth:".3",opacity:".25"}),(0,u.jsx)("line",{x1:"18",y1:"2",x2:"2",y2:"14",stroke:v,strokeWidth:".3",opacity:".25"})]});case"video":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("path",{d:"M8.5 5.5v5l4.5-2.5z",stroke:v,strokeWidth:F,fill:v,opacity:".15"})]});case"table":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"2",width:"18",height:"12",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("line",{x1:"1",y1:"5.5",x2:"19",y2:"5.5",stroke:v,strokeWidth:".3",opacity:".25"}),(0,u.jsx)("line",{x1:"1",y1:"9",x2:"19",y2:"9",stroke:v,strokeWidth:".3",opacity:".25"}),(0,u.jsx)("line",{x1:"7",y1:"2",x2:"7",y2:"14",stroke:v,strokeWidth:".3",opacity:".25"}),(0,u.jsx)("line",{x1:"13",y1:"2",x2:"13",y2:"14",stroke:v,strokeWidth:".3",opacity:".25"})]});case"grid":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"2",width:"7",height:"5.5",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"11.5",y:"2",width:"7",height:"5.5",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"1.5",y:"9.5",width:"7",height:"5.5",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"11.5",y:"9.5",width:"7",height:"5.5",rx:"1",stroke:v,strokeWidth:F})]});case"list":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"3.5",cy:"4.5",r:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6.5",y:"4",width:"10",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"3.5",cy:"8",r:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6.5",y:"7.5",width:"8",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"3.5",cy:"11.5",r:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6.5",y:"11",width:"11",height:"1",rx:".5",fill:v,opacity:".2"})]});case"chart":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"9",width:"2.5",height:"4",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"7",y:"6",width:"2.5",height:"7",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"11",y:"3",width:"2.5",height:"10",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"15",y:"5",width:"2.5",height:"8",rx:".5",fill:v,opacity:".2"})]});case"accordion":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"2",width:"17",height:"4",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"3",y:"3.5",width:"6",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"1.5",y:"7.5",width:"17",height:"3",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"1.5",y:"12",width:"17",height:"3",rx:"1",stroke:v,strokeWidth:F})]});case"carousel":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"10",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("path",{d:"M1.5 7L3 8.5 1.5 10",stroke:v,strokeWidth:F,opacity:".35"}),(0,u.jsx)("path",{d:"M18.5 7L17 8.5 18.5 10",stroke:v,strokeWidth:F,opacity:".35"}),(0,u.jsx)("circle",{cx:"8.5",cy:"14",r:".6",fill:v,opacity:".35"}),(0,u.jsx)("circle",{cx:"10",cy:"14",r:".6",fill:v,opacity:".15"}),(0,u.jsx)("circle",{cx:"11.5",cy:"14",r:".6",fill:v,opacity:".15"})]});case"button":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"5",width:"14",height:"6",rx:"2",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6.5",y:"7.5",width:"7",height:"1",rx:".5",fill:v,opacity:".25"})]});case"input":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4",width:"5.5",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"2",y:"6.5",width:"16",height:"5.5",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"3.5",y:"8.5",width:"7",height:"1",rx:".5",fill:v,opacity:".12"})]});case"search":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4.5",width:"16",height:"7",rx:"3.5",stroke:v,strokeWidth:F}),(0,u.jsx)("circle",{cx:"6",cy:"8",r:"2",stroke:v,strokeWidth:F,opacity:".3"}),(0,u.jsx)("line",{x1:"7.5",y1:"9.5",x2:"9",y2:"11",stroke:v,strokeWidth:F,opacity:".3"}),(0,u.jsx)("rect",{x:"9.5",y:"7.5",width:"6",height:"1",rx:".5",fill:v,opacity:".12"})]});case"form":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1.5",width:"5.5",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"2",y:"3.5",width:"16",height:"3",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"2",y:"8",width:"7",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"2",y:"10",width:"16",height:"3",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"12",y:"14",width:"6",height:"2",rx:".75",stroke:v,strokeWidth:F})]});case"tabs":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"5",width:"18",height:"10",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"1",y:"2",width:"6",height:"3.5",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"2.5",y:"3.25",width:"3",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"2",width:"6",height:"3.5",rx:".75",stroke:v,strokeWidth:F})]});case"dropdown":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"4",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"3.5",y:"3.5",width:"7",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("path",{d:"M15 3.5l1.5 1.5L18 3.5",stroke:v,strokeWidth:F,opacity:".3"}),(0,u.jsx)("rect",{x:"2",y:"7",width:"16",height:"7",rx:"1",stroke:v,strokeWidth:F,strokeDasharray:"2 1",opacity:".3"})]});case"toggle":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"4",y:"5",width:"12",height:"6",rx:"3",stroke:v,strokeWidth:F}),(0,u.jsx)("circle",{cx:"13",cy:"8",r:"2",fill:v,opacity:".3"})]});case"avatar":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"10",cy:"8",r:"6",stroke:v,strokeWidth:F}),(0,u.jsx)("circle",{cx:"10",cy:"6.5",r:"2",stroke:v,strokeWidth:F}),(0,u.jsx)("path",{d:"M6.5 13c0-2 1.5-3.5 3.5-3.5s3.5 1.5 3.5 3.5",stroke:v,strokeWidth:F})]});case"badge":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"5",width:"14",height:"6",rx:"3",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6",y:"7.5",width:"8",height:"1",rx:".5",fill:v,opacity:".25"})]});case"breadcrumb":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"7",width:"3.5",height:"1",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("path",{d:"M6.5 7l1 1-1 1",stroke:v,strokeWidth:F,opacity:".2"}),(0,u.jsx)("rect",{x:"9",y:"7",width:"3.5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("path",{d:"M14 7l1 1-1 1",stroke:v,strokeWidth:F,opacity:".2"}),(0,u.jsx)("rect",{x:"16.5",y:"7",width:"2",height:"1",rx:".5",fill:v,opacity:".15"})]});case"pagination":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"5.5",width:"3.5",height:"5",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6.5",y:"5.5",width:"3.5",height:"5",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"11",y:"5.5",width:"3.5",height:"5",rx:"1",fill:v,opacity:".15",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"15.5",y:"5.5",width:"3.5",height:"5",rx:"1",stroke:v,strokeWidth:F})]});case"progress":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"7",width:"16",height:"2",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"2",y:"7",width:"10",height:"2",rx:"1",fill:v,opacity:".2"})]});case"toast":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4",width:"16",height:"8",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("circle",{cx:"5",cy:"8",r:"1.5",stroke:v,strokeWidth:F,opacity:".3"}),(0,u.jsx)("rect",{x:"8",y:"6.5",width:"7",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"8",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".12"})]});case"tooltip":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"3",width:"14",height:"7",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"5.5",y:"5.5",width:"9",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("path",{d:"M9 10l1 2.5 1-2.5",stroke:v,strokeWidth:F})]});case"pricing":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"14",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6",y:"3",width:"8",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"5.5",width:"6",height:"2",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"5",y:"9",width:"10",height:"1",rx:".5",fill:v,opacity:".1"}),(0,u.jsx)("rect",{x:"5",y:"11",width:"10",height:"1",rx:".5",fill:v,opacity:".1"}),(0,u.jsx)("rect",{x:"6",y:"13",width:"8",height:"1.5",rx:".5",fill:v,opacity:".2"})]});case"testimonial":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"14",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("text",{x:"4",y:"5.5",fontSize:"4",fill:v,opacity:".2",fontFamily:"serif",children:"\u201C"}),(0,u.jsx)("rect",{x:"4",y:"7",width:"12",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"4",y:"9",width:"9",height:"1",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("circle",{cx:"5.5",cy:"12.5",r:"1.5",stroke:v,strokeWidth:F,opacity:".25"}),(0,u.jsx)("rect",{x:"8",y:"12",width:"5",height:"1",rx:".5",fill:v,opacity:".15"})]});case"cta":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"2",width:"18",height:"12",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"5",y:"4.5",width:"10",height:"1.5",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"6",y:"7.5",width:"8",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"7",y:"10",width:"6",height:"2.5",rx:"1",stroke:v,strokeWidth:F})]});case"alert":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"4",width:"16",height:"8",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("circle",{cx:"6",cy:"8",r:"2",stroke:v,strokeWidth:F,opacity:".3"}),(0,u.jsx)("line",{x1:"6",y1:"7",x2:"6",y2:"8.5",stroke:v,strokeWidth:"0.6",opacity:".5"}),(0,u.jsx)("circle",{cx:"6",cy:"9.3",r:".3",fill:v,opacity:".5"}),(0,u.jsx)("rect",{x:"9.5",y:"7",width:"6",height:"1",rx:".5",fill:v,opacity:".2"})]});case"banner":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"5",width:"18",height:"6",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"4",y:"7.5",width:"8",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"14",y:"7",width:"3.5",height:"2",rx:".75",stroke:v,strokeWidth:F})]});case"stat":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"12",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6",y:"4.5",width:"8",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"5",y:"7",width:"10",height:"2.5",rx:".5",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"7",y:"11",width:"6",height:"1",rx:".5",fill:v,opacity:".12"})]});case"stepper":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"4",cy:"8",r:"2",fill:v,opacity:".2",stroke:v,strokeWidth:F}),(0,u.jsx)("line",{x1:"6",y1:"8",x2:"8",y2:"8",stroke:v,strokeWidth:".4",opacity:".3"}),(0,u.jsx)("circle",{cx:"10",cy:"8",r:"2",stroke:v,strokeWidth:F}),(0,u.jsx)("line",{x1:"12",y1:"8",x2:"14",y2:"8",stroke:v,strokeWidth:".4",opacity:".3"}),(0,u.jsx)("circle",{cx:"16",cy:"8",r:"2",stroke:v,strokeWidth:F})]});case"tag":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"5",width:"14",height:"6",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"5.5",y:"7.5",width:"6",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("line",{x1:"14",y1:"6.5",x2:"15.5",y2:"9.5",stroke:v,strokeWidth:F,opacity:".2"}),(0,u.jsx)("line",{x1:"15.5",y1:"6.5",x2:"14",y2:"9.5",stroke:v,strokeWidth:F,opacity:".2"})]});case"rating":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("path",{d:"M4 5.5l1 2 2.2.3-1.6 1.5.4 2.2L4 10.3l-2 1.2.4-2.2L.8 7.8 3 7.5z",fill:v,opacity:".25"}),(0,u.jsx)("path",{d:"M10 5.5l1 2 2.2.3-1.6 1.5.4 2.2L10 10.3l-2 1.2.4-2.2L6.8 7.8 9 7.5z",fill:v,opacity:".25"}),(0,u.jsx)("path",{d:"M16 5.5l1 2 2.2.3-1.6 1.5.4 2.2L16 10.3l-2 1.2.4-2.2-1.6-1.5 2.2-.3z",stroke:v,strokeWidth:F,opacity:".25"})]});case"map":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("line",{x1:"2",y1:"6",x2:"18",y2:"10",stroke:v,strokeWidth:".3",opacity:".15"}),(0,u.jsx)("line",{x1:"7",y1:"2",x2:"11",y2:"14",stroke:v,strokeWidth:".3",opacity:".15"}),(0,u.jsx)("path",{d:"M10 5c-1.7 0-3 1.3-3 3 0 2.5 3 5 3 5s3-2.5 3-5c0-1.7-1.3-3-3-3z",fill:v,opacity:".15",stroke:v,strokeWidth:F})]});case"timeline":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("line",{x1:"5",y1:"2",x2:"5",y2:"14",stroke:v,strokeWidth:".4",opacity:".25"}),(0,u.jsx)("circle",{cx:"5",cy:"4",r:"1.5",fill:v,opacity:".2",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"8",y:"3",width:"8",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("circle",{cx:"5",cy:"8.5",r:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"8",y:"7.5",width:"6",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("circle",{cx:"5",cy:"13",r:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"8",y:"12",width:"7",height:"1",rx:".5",fill:v,opacity:".15"})]});case"fileUpload":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"12",rx:"1.5",stroke:v,strokeWidth:F,strokeDasharray:"2 1"}),(0,u.jsx)("path",{d:"M10 10V5.5m0 0L7.5 8m2.5-2.5L12.5 8",stroke:v,strokeWidth:F,opacity:".3"}),(0,u.jsx)("rect",{x:"7",y:"11.5",width:"6",height:"1",rx:".5",fill:v,opacity:".15"})]});case"codeBlock":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("circle",{cx:"4",cy:"4",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("circle",{cx:"5.5",cy:"4",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("circle",{cx:"7",cy:"4",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("rect",{x:"4",y:"7",width:"7",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"6",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"4",y:"11",width:"8",height:"1",rx:".5",fill:v,opacity:".12"})]});case"calendar":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"3",width:"16",height:"12",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("line",{x1:"2",y1:"6.5",x2:"18",y2:"6.5",stroke:v,strokeWidth:".4",opacity:".25"}),(0,u.jsx)("rect",{x:"5",y:"4",width:"1",height:"1.5",rx:".3",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"14",y:"4",width:"1",height:"1.5",rx:".3",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"7",cy:"9",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"9",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"13",cy:"9",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("circle",{cx:"7",cy:"12",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"12",r:".6",fill:v,opacity:".2"})]});case"notification":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"3",width:"16",height:"10",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("circle",{cx:"5.5",cy:"8",r:"2",stroke:v,strokeWidth:F,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"6",width:"6",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"8.5",width:"4.5",height:"1",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("circle",{cx:"16.5",cy:"4.5",r:"1.5",fill:v,opacity:".25"})]});case"productCard":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"1",width:"14",height:"14",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"3",y:"1",width:"14",height:"6",rx:"1",fill:v,opacity:".04"}),(0,u.jsx)("rect",{x:"5",y:"8.5",width:"7",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"5",y:"10.5",width:"4",height:"1.5",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"12",y:"12",width:"4",height:"2",rx:".75",stroke:v,strokeWidth:F})]});case"profile":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"10",cy:"5",r:"3",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"5",y:"10",width:"10",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"12.5",width:"6",height:"1",rx:".5",fill:v,opacity:".12"})]});case"drawer":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"9",y:"1",width:"10",height:"14",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"10.5",y:"4",width:"5",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"10.5",y:"6.5",width:"7",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"10.5",y:"9",width:"6",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"1",y:"1",width:"7",height:"14",rx:"1",stroke:v,strokeWidth:F,opacity:".15"})]});case"popover":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"2",width:"14",height:"9",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"5",y:"4.5",width:"8",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"5",y:"7",width:"6",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("path",{d:"M9 11l1 2.5 1-2.5",stroke:v,strokeWidth:F})]});case"logo":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"3",width:"10",height:"10",rx:"2",stroke:v,strokeWidth:F}),(0,u.jsx)("path",{d:"M5 9.5l2-4 2 4",stroke:v,strokeWidth:F,opacity:".3"}),(0,u.jsx)("rect",{x:"14",y:"6",width:"4",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"14",y:"8.5",width:"3",height:"1",rx:".5",fill:v,opacity:".12"})]});case"faq":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("text",{x:"2.5",y:"5.5",fontSize:"4",fill:v,opacity:".3",fontWeight:"bold",children:"?"}),(0,u.jsx)("rect",{x:"7",y:"3",width:"10",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"5.5",width:"8",height:"1",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("text",{x:"2.5",y:"11.5",fontSize:"4",fill:v,opacity:".3",fontWeight:"bold",children:"?"}),(0,u.jsx)("rect",{x:"7",y:"9",width:"9",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"7",y:"11.5",width:"7",height:"1",rx:".5",fill:v,opacity:".12"})]});case"gallery":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"1.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"7.5",y:"1.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"13.5",y:"1.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"1.5",y:"9.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"7.5",y:"9.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"13.5",y:"9.5",width:"5",height:"5",rx:".75",stroke:v,strokeWidth:F})]});case"checkbox":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"5",y:"4",width:"8",height:"8",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("path",{d:"M7.5 8l1.5 1.5 3-3",stroke:v,strokeWidth:F,opacity:".35"})]});case"radio":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"10",cy:"8",r:"4",stroke:v,strokeWidth:F}),(0,u.jsx)("circle",{cx:"10",cy:"8",r:"2",fill:v,opacity:".3"})]});case"slider":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"7.5",width:"16",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"2",y:"7.5",width:"10",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("circle",{cx:"12",cy:"8",r:"2.5",stroke:v,strokeWidth:F})]});case"datePicker":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"5",rx:"1",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"3.5",y:"3",width:"5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"14",y:"2.5",width:"2.5",height:"2",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("rect",{x:"2",y:"7",width:"16",height:"8",rx:"1",stroke:v,strokeWidth:F,strokeDasharray:"2 1",opacity:".3"}),(0,u.jsx)("circle",{cx:"6",cy:"10",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"10",r:".6",fill:v,opacity:".3"}),(0,u.jsx)("circle",{cx:"14",cy:"10",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"6",cy:"13",r:".6",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"13",r:".6",fill:v,opacity:".2"})]});case"skeleton":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"16",height:"3",rx:"1",fill:v,opacity:".08"}),(0,u.jsx)("rect",{x:"2",y:"7",width:"10",height:"2",rx:".75",fill:v,opacity:".08"}),(0,u.jsx)("rect",{x:"2",y:"11",width:"13",height:"2",rx:".75",fill:v,opacity:".08"})]});case"chip":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"1.5",y:"5",width:"10",height:"6",rx:"3",fill:v,opacity:".08",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"4",y:"7.5",width:"4",height:"1",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("line",{x1:"9.5",y1:"6.5",x2:"10.5",y2:"9.5",stroke:v,strokeWidth:F,opacity:".2"}),(0,u.jsx)("line",{x1:"10.5",y1:"6.5",x2:"9.5",y2:"9.5",stroke:v,strokeWidth:F,opacity:".2"}),(0,u.jsx)("rect",{x:"13",y:"5",width:"5.5",height:"6",rx:"3",stroke:v,strokeWidth:F,opacity:".25"})]});case"icon":return(0,u.jsx)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:(0,u.jsx)("path",{d:"M10 3l1.5 3 3.5.5-2.5 2.5.5 3.5L10 11l-3 1.5.5-3.5L5 6.5l3.5-.5z",stroke:v,strokeWidth:F,opacity:".3"})});case"spinner":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"10",cy:"8",r:"5",stroke:v,strokeWidth:F,opacity:".12"}),(0,u.jsx)("path",{d:"M10 3a5 5 0 0 1 5 5",stroke:v,strokeWidth:F,opacity:".35",strokeLinecap:"round"})]});case"feature":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"2",width:"5",height:"5",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("path",{d:"M4.5 3.5v3m-1.5-1.5h3",stroke:v,strokeWidth:F,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"2.5",width:"8",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"5.5",width:"6",height:"1",rx:".5",fill:v,opacity:".12"}),(0,u.jsx)("rect",{x:"2",y:"10",width:"5",height:"5",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"9",y:"10.5",width:"7",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"9",y:"13.5",width:"5",height:"1",rx:".5",fill:v,opacity:".12"})]});case"team":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("circle",{cx:"5",cy:"5",r:"2.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"2.5",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"15",cy:"5",r:"2.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"12.5",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("circle",{cx:"10",cy:"5",r:"2.5",stroke:v,strokeWidth:F,opacity:".5"}),(0,u.jsx)("rect",{x:"7.5",y:"9",width:"5",height:"1",rx:".5",fill:v,opacity:".15"}),(0,u.jsx)("rect",{x:"4",y:"12",width:"12",height:"1",rx:".5",fill:v,opacity:".1"})]});case"login":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"3",y:"1",width:"14",height:"14",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6",y:"3",width:"8",height:"1.5",rx:".5",fill:v,opacity:".25"}),(0,u.jsx)("rect",{x:"5",y:"5.5",width:"10",height:"3",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"5",y:"9.5",width:"10",height:"3",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"6.5",y:"13.5",width:"7",height:"2",rx:".75",fill:v,opacity:".2"})]});case"contact":return(0,u.jsxs)("svg",{viewBox:"0 0 20 16",width:"20",height:"16",fill:"none",children:[(0,u.jsx)("rect",{x:"2",y:"1",width:"16",height:"14",rx:"1.5",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"4",y:"3",width:"5",height:"1",rx:".5",fill:v,opacity:".2"}),(0,u.jsx)("rect",{x:"4",y:"5",width:"12",height:"2.5",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"4",y:"8.5",width:"12",height:"4",rx:".75",stroke:v,strokeWidth:F}),(0,u.jsx)("rect",{x:"11",y:"13.5",width:"5",height:"1.5",rx:".5",fill:v,opacity:".2"})]});default:return null}}function Hw({activeType:e,onSelect:t,onDragStart:n,scrollRef:o,fadeClass:r,blankCanvas:i}){return(0,u.jsx)("div",{ref:o,className:`${$.placeScroll} ${r||""}`,children:Tm.map(s=>(0,u.jsxs)("div",{className:$.paletteSection,children:[(0,u.jsx)("div",{className:$.paletteSectionTitle,children:s.section}),s.items.map(l=>(0,u.jsxs)("button",{type:"button","aria-pressed":e===l.type,className:`${$.paletteItem} ${e===l.type?$.active:""} ${i?$.wireframe:""}`,onClick:()=>t(l.type),onMouseDown:a=>{a.button===0&&n(l.type,a)},children:[(0,u.jsx)("span",{className:$.paletteItemIcon,"aria-hidden":"true",children:(0,u.jsx)(zw,{type:l.type})}),(0,u.jsx)("span",{className:$.paletteItemLabel,children:l.label})]},l.type))]},s.section))})}function Ww({value:e,suffix:t}){let[n,o]=(0,Kt.useState)(null),[r,i]=(0,Kt.useState)(t),[s,l]=(0,Kt.useState)("up"),a=(0,Kt.useRef)(e),c=(0,Kt.useRef)(t),d=(0,Kt.useRef)(),m=n!==null&&r!==t;return(0,Kt.useEffect)(()=>{if(e!==a.current){if(e===0){a.current=e,c.current=t,o(null);return}l(e>a.current?"up":"down"),o(a.current),i(c.current),a.current=e,c.current=t,clearTimeout(d.current),d.current=Ne(()=>o(null),250)}else c.current=t},[e,t]),n===null?(0,u.jsxs)(u.Fragment,{children:[e,t?` ${t}`:""]}):m?(0,u.jsxs)("span",{className:$.rollingWrap,children:[(0,u.jsxs)("span",{style:{visibility:"hidden"},children:[e," ",t]}),(0,u.jsxs)("span",{className:`${$.rollingNum} ${s==="up"?$.exitUp:$.exitDown}`,children:[n," ",r]},`o${n}-${e}`),(0,u.jsxs)("span",{className:`${$.rollingNum} ${s==="up"?$.enterUp:$.enterDown}`,children:[e," ",t]},`n${e}`)]}):(0,u.jsxs)(u.Fragment,{children:[(0,u.jsxs)("span",{className:$.rollingWrap,children:[(0,u.jsx)("span",{style:{visibility:"hidden"},children:e}),(0,u.jsx)("span",{className:`${$.rollingNum} ${s==="up"?$.exitUp:$.exitDown}`,children:n},`o${n}-${e}`),(0,u.jsx)("span",{className:`${$.rollingNum} ${s==="up"?$.enterUp:$.enterDown}`,children:e},`n${e}`)]}),t?` ${t}`:""]})}function Ny({activeType:e,onSelect:t,isDarkMode:n,sectionCount:o,onDetectSections:r,visible:i,onExited:s,placementCount:l,onClearPlacements:a,onDragStart:c,blankCanvas:d,onBlankCanvasChange:m,wireframePurpose:f,onWireframePurposeChange:k,placement:w="above",Tooltip:C}){let{ref:S,mounted:g}=nu(i,{onExited:s}),[x,h]=(0,Kt.useState)(!1),[E,O]=(0,Kt.useState)(!0),j=(0,Kt.useRef)(0),A=(0,Kt.useRef)(""),Y=(0,Kt.useRef)(null),[K,q]=(0,Kt.useState)(""),Z=l>0||o>0,Ie=l+o;if(Ie>0&&(j.current=Ie,A.current=d?Ie===1?"Component":"Components":Ie===1?"Change":"Changes"),(0,Kt.useEffect)(()=>{if(Z)x?O(!1):(O(!0),h(!0),Zi(()=>{Zi(()=>{O(!1)})}));else{O(!0);let me=Ne(()=>h(!1),300);return()=>clearTimeout(me)}},[Z]),(0,Kt.useEffect)(()=>{if(!i)return;let me=Y.current;if(!me)return;let Me=()=>q(Bw(me));me.addEventListener("scroll",Me,{passive:!0});let B=new ResizeObserver(Me);return B.observe(me),()=>{me.removeEventListener("scroll",Me),B.disconnect()}},[i]),!g)return null;let ye=[];return l>0&&ye.push("placed"),o>0&&ye.push("captured"),(0,u.jsxs)("div",{className:`${$.palette} ${w==="topRight"?$.paletteTopRight:""} ${n?"":$.light}`,ref:me=>{S.current=me,me?.toggleAttribute("inert",!i)},"aria-hidden":!i,"data-feedback-toolbar":!0,"data-agentation-palette":!0,onClick:me=>me.stopPropagation(),onMouseDown:me=>me.stopPropagation(),children:[(0,u.jsxs)("div",{className:$.paletteHeader,children:[(0,u.jsx)("div",{className:$.paletteHeaderTitle,children:"Layout Mode"}),(0,u.jsxs)("div",{className:$.paletteHeaderDesc,children:["Rearrange and resize existing elements, add new components, and explore layout ideas. Agent results may vary."," ",(0,u.jsx)("a",{href:"https://agentation.com/features#layout-mode",target:"_blank",rel:"noopener noreferrer",children:"Learn more."})]})]}),(0,u.jsxs)("button",{type:"button","aria-pressed":d,className:`${$.canvasToggle} ${d?$.active:""}`,onClick:()=>m(!d),children:[(0,u.jsx)("span",{className:$.canvasToggleIcon,"aria-hidden":"true",children:(0,u.jsxs)("svg",{viewBox:"0 0 14 14",width:"14",height:"14",fill:"none",children:[(0,u.jsx)("rect",{x:"1",y:"1",width:"12",height:"12",rx:"2",stroke:"currentColor",strokeWidth:"1"}),(0,u.jsx)("circle",{cx:"4.5",cy:"4.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"7",cy:"4.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"9.5",cy:"4.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"4.5",cy:"7",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"7",cy:"7",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"9.5",cy:"7",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"4.5",cy:"9.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"7",cy:"9.5",r:"0.8",fill:"currentColor",opacity:".6"}),(0,u.jsx)("circle",{cx:"9.5",cy:"9.5",r:"0.8",fill:"currentColor",opacity:".6"})]})}),(0,u.jsx)("span",{className:$.canvasToggleLabel,children:"Wireframe New Page"})]}),(0,u.jsx)("div",{className:`${$.wireframePurposeWrap} ${d?"":$.collapsed}`,"aria-hidden":!d,ref:me=>{me?.toggleAttribute("inert",!d)},children:(0,u.jsx)("div",{className:$.wireframePurposeInner,children:(0,u.jsx)("textarea",{className:$.wireframePurposeInput,placeholder:"Describe this page to provide additional context for your agent.",value:f,onChange:me=>k(me.target.value),rows:2})})}),(0,u.jsx)(Hw,{activeType:e,onSelect:t,onDragStart:c,scrollRef:Y,fadeClass:K,blankCanvas:d}),x&&(0,u.jsx)("div",{className:`${$.paletteFooterWrap} ${E?$.footerHidden:""}`,children:(0,u.jsx)("div",{className:$.paletteFooterInner,children:(0,u.jsx)("div",{className:$.paletteFooterInnerContent,children:(0,u.jsxs)("div",{className:$.paletteFooter,children:[(0,u.jsx)("span",{className:$.paletteFooterCount,children:(0,u.jsx)(Ww,{value:j.current,suffix:A.current})}),(0,u.jsx)("button",{className:$.paletteFooterClear,onClick:a,children:"Clear"})]})})})})]})}var ke=ae(bt());var Uw=new Set(["nav","header","main","section","article","footer","aside"]),Lm={banner:"Header",navigation:"Navigation",main:"Main Content",contentinfo:"Footer",complementary:"Sidebar",region:"Section"},Iy={nav:"Navigation",header:"Header",main:"Main Content",section:"Section",article:"Article",footer:"Footer",aside:"Sidebar"},jw=new Set(["script","style","noscript","link","meta"]),Yw=40;function Dy(e){let t=e;for(;t&&t!==document.body&&t!==document.documentElement;){let n=window.getComputedStyle(t).position;if(n==="fixed"||n==="sticky")return!0;t=t.parentElement}return!1}function Cr(e){let t=e.tagName.toLowerCase();if(["nav","header","footer","main"].includes(t)&&document.querySelectorAll(t).length===1)return t;if(e.id)return`#${CSS.escape(e.id)}`;if(e.className&&typeof e.className=="string"){let r=e.className.split(/\s+/).filter(i=>i.length>0).find(i=>i.length>2&&!/^[a-zA-Z0-9]{6,}$/.test(i)&&!/^[a-z]{1,2}$/.test(i));if(r){let i=`${t}.${CSS.escape(r)}`;if(document.querySelectorAll(i).length===1)return i}}let n=e.parentElement;if(n){let r=Array.from(n.children).indexOf(e)+1;return`${n===document.body?"body":Cr(n)} > ${t}:nth-child(${r})`}return t}function Fl(e){let t=e.tagName.toLowerCase(),n=e.getAttribute("aria-label");if(n)return n;let o=e.getAttribute("role");if(o&&Lm[o])return Lm[o];if(Iy[t])return Iy[t];let r=e.querySelector("h1, h2, h3, h4, h5, h6");if(r){let s=r.textContent?.trim();if(s&&s.length<=50)return s;if(s)return s.slice(0,47)+"..."}let{name:i}=kr(e);return i.charAt(0).toUpperCase()+i.slice(1)}function Ay(e){let t=e.className;return typeof t!="string"||!t?null:t.split(/\s+/).map(o=>o.replace(/[_][a-zA-Z0-9]{5,}.*$/,"")).find(o=>o.length>2&&!/^[a-z]{1,2}$/.test(o))||null}function $y(e){let t=e.textContent?.trim();if(!t)return null;let n=t.replace(/\s+/g," ");return n.length<=30?n:n.slice(0,30)+"\u2026"}function Fy(){let e=document.querySelector("main")||document.body,t=Array.from(e.children),n=t;e!==document.body&&t.length<3&&(n=Array.from(document.body.children));let o=[];return n.forEach((r,i)=>{if(!(r instanceof HTMLElement))return;let s=r.tagName.toLowerCase();if(jw.has(s)||r.hasAttribute("data-feedback-toolbar")||r.closest("[data-feedback-toolbar]"))return;let l=window.getComputedStyle(r);if(l.display==="none"||l.visibility==="hidden")return;let a=r.getBoundingClientRect();if(a.height<Yw)return;let c=Uw.has(s),d=r.getAttribute("role")&&Lm[r.getAttribute("role")],m=s==="div"&&a.height>=60;if(!c&&!d&&!m)return;let f=window.scrollY,k=Dy(r),w={x:a.x,y:k?a.y:a.y+f,width:a.width,height:a.height};o.push({id:`rs-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,label:Fl(r),tagName:s,selector:Cr(r),role:r.getAttribute("role"),className:Ay(r),textSnippet:$y(r),originalRect:w,currentRect:{...w},originalIndex:i,isFixed:k})}),o}function Oy(e){let t=window.scrollY,n=e.getBoundingClientRect(),o=Dy(e),r={x:n.x,y:o?n.y:n.y+t,width:n.width,height:n.height},i=e.parentElement,s=0;return i&&(s=Array.from(i.children).indexOf(e)),{id:`rs-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,label:Fl(e),tagName:e.tagName.toLowerCase(),selector:Cr(e),role:e.getAttribute("role"),className:Ay(e),textSnippet:$y(e),originalRect:r,currentRect:{...r},originalIndex:s,isFixed:o}}var Ke=ae(Be()),By={bg:"rgba(59, 130, 246, 0.08)",border:"rgba(59, 130, 246, 0.5)",pill:"#3b82f6"},zy=["nw","n","ne","e","se","s","sw","w"],ou=24,Hy=16,ru=5;function Wy(e,t,n,o){let r=1/0,i=1/0,s=e.x,l=e.x+e.width,a=e.x+e.width/2,c=e.y,d=e.y+e.height,m=e.y+e.height/2,f=[];for(let A of t)n.has(A.id)||f.push(A.currentRect);o&&f.push(...o);for(let A of f){let Y=A.x,K=A.x+A.width,q=A.x+A.width/2,Z=A.y,Ie=A.y+A.height,ye=A.y+A.height/2;for(let me of[s,l,a])for(let Me of[Y,K,q]){let B=Me-me;Math.abs(B)<ru&&Math.abs(B)<Math.abs(r)&&(r=B)}for(let me of[c,d,m])for(let Me of[Z,Ie,ye]){let B=Me-me;Math.abs(B)<ru&&Math.abs(B)<Math.abs(i)&&(i=B)}}let k=Math.abs(r)<ru?r:0,w=Math.abs(i)<ru?i:0,C=[],S=new Set,g=s+k,x=l+k,h=a+k,E=c+w,O=d+w,j=m+w;for(let A of f){let Y=A.x,K=A.x+A.width,q=A.x+A.width/2,Z=A.y,Ie=A.y+A.height,ye=A.y+A.height/2;for(let me of[Y,q,K])for(let Me of[g,h,x])if(Math.abs(Me-me)<.5){let B=`x:${Math.round(me)}`;S.has(B)||(S.add(B),C.push({axis:"x",pos:me}))}for(let me of[Z,ye,Ie])for(let Me of[E,j,O])if(Math.abs(Me-me)<.5){let B=`y:${Math.round(me)}`;S.has(B)||(S.add(B),C.push({axis:"y",pos:me}))}}return{dx:k,dy:w,guides:C}}var Vw=new Set(["script","style","noscript","link","meta","br","hr"]);function Uy(e){let t=e;for(;t&&t!==document.body&&t!==document.documentElement;){if(t.closest("[data-feedback-toolbar]"))return null;if(Vw.has(t.tagName.toLowerCase())){t=t.parentElement;continue}let n=t.getBoundingClientRect();if(n.width>=Hy&&n.height>=Hy)return t;t=t.parentElement}return null}function jy({rearrangeState:e,onChange:t,isDarkMode:n,exiting:o,className:r,blankCanvas:i,extraSnapRects:s,onSelectionChange:l,deselectSignal:a,onDragMove:c,onDragEnd:d,clearing:m}){let{sections:f}=e,k=(0,ke.useRef)(e);k.current=e;let[w,C]=(0,ke.useState)(new Set);(0,ke.useEffect)(()=>{m&&C(new Set)},[m]);let S=(0,ke.useRef)(a);(0,ke.useEffect)(()=>{a!==S.current&&(S.current=a,C(new Set))},[a]);let[g,x]=(0,ke.useState)(null),[h,E]=(0,ke.useState)(!1),O=(0,ke.useRef)(!1),j=(0,ke.useCallback)(L=>{let D=f.find(z=>z.id===L);D&&(O.current=!!D.note,x(L),E(!1))},[f]),A=(0,ke.useCallback)(()=>{g&&(E(!0),Ne(()=>{x(null),E(!1)},150))},[g]),Y=(0,ke.useCallback)(L=>{g&&(t({...e,sections:f.map(D=>D.id===g?{...D,note:L.trim()||void 0}:D)}),A())},[g,f,e,t,A]);(0,ke.useEffect)(()=>{o&&g&&A()},[o]);let[K,q]=(0,ke.useState)(new Set),Z=(0,ke.useRef)(new Map),[Ie,ye]=(0,ke.useState)(null),[me,Me]=(0,ke.useState)(null),[B,dt]=(0,ke.useState)([]),[ut,ue]=(0,ke.useState)(0),$e=(0,ke.useRef)(null),Fe=(0,ke.useRef)(new Set),vt=(0,ke.useRef)(new Map),[Ct,Ht]=(0,ke.useState)(new Map),[Zt,ci]=(0,ke.useState)(new Map),mo=(0,ke.useRef)(new Set),qo=(0,ke.useRef)(new Map),po=(0,ke.useRef)(l);po.current=l;let fo=(0,ke.useRef)(c);fo.current=c;let ze=(0,ke.useRef)(d);ze.current=d,(0,ke.useEffect)(()=>{i&&C(new Set)},[i]);let[en,go]=(0,ke.useState)(()=>!e.sections.some(L=>{let D=L.originalRect,z=L.currentRect;return Math.abs(D.x-z.x)>1||Math.abs(D.y-z.y)>1||Math.abs(D.width-z.width)>1||Math.abs(D.height-z.height)>1}));(0,ke.useEffect)(()=>{if(!en){let L=Ne(()=>go(!0),380);return()=>clearTimeout(L)}},[]);let Zn=(0,ke.useRef)(new Set);(0,ke.useEffect)(()=>{Zn.current=new Set(f.map(L=>L.selector))},[f]),(0,ke.useEffect)(()=>{let L=()=>ue(window.scrollY);return L(),window.addEventListener("scroll",L,{passive:!0}),window.addEventListener("resize",L,{passive:!0}),()=>{window.removeEventListener("scroll",L),window.removeEventListener("resize",L)}},[]),(0,ke.useEffect)(()=>{let L=D=>{if($e.current){ye(null);return}let z=document.elementFromPoint(D.clientX,D.clientY);if(!z){ye(null);return}if(z.closest("[data-feedback-toolbar]")){ye(null);return}if(z.closest("[data-design-placement]")){ye(null);return}if(z.closest("[data-annotation-popup]")){ye(null);return}let U=Uy(z);if(!U){ye(null);return}for(let ce of Zn.current)try{let R=document.querySelector(ce);if(R&&(R===U||U.contains(R))){ye(null);return}}catch{}let se=U.getBoundingClientRect();ye({x:se.x,y:se.y,w:se.width,h:se.height})};return document.addEventListener("mousemove",L,{passive:!0}),()=>document.removeEventListener("mousemove",L)},[f]),(0,ke.useEffect)(()=>{let L=document.body.style.userSelect;return document.body.style.webkitUserSelect="none",document.body.style.userSelect="none",()=>{document.body.style.webkitUserSelect=L,document.body.style.userSelect=L}},[]),(0,ke.useEffect)(()=>{let L=D=>{if($e.current||D.button!==0)return;let z=D.composedPath()[0]??D.target;if(!z||z.closest("[data-feedback-toolbar]")||z.closest("[data-design-placement]")||z.closest("[data-annotation-popup]"))return;let U=Uy(z),se=!1;if(U)for(let R of Zn.current)try{let le=document.querySelector(R);if(le&&(le===U||U.contains(le))){se=!0;break}}catch{}let ce=!!(D.shiftKey||D.metaKey||D.ctrlKey);if(U&&!se){D.preventDefault(),D.stopPropagation();let R=Oy(U),le=[...f,R],we=[...e.originalOrder,R.id];t({...e,sections:le,originalOrder:we});let He=new Set([R.id]);C(He),po.current?.(He,ce),ye(null);let Je=D.clientX,it=D.clientY,We={x:R.currentRect.x,y:R.currentRect.y},Ze=R.originalRect,Te=!1,wt=0,ht=0;$e.current="move";let at=Ve=>{let et=Ve.clientX-Je,ft=Ve.clientY-it;if(!Te&&(Math.abs(et)>2||Math.abs(ft)>2)&&(Te=!0),!Te)return;let Wt={x:We.x+et,y:We.y+ft,width:R.currentRect.width,height:R.currentRect.height},sn=Wy(Wt,le,new Set([R.id]),s);dt(sn.guides);let ho=et+sn.dx,Xt=ft+sn.dy;wt=ho,ht=Xt;let Io=Lt().querySelector(`[data-rearrange-section="${R.id}"]`);Io&&(Io.style.transform=`translate(${ho}px, ${Xt}px)`),Ht(new Map([[R.id,{x:We.x+ho,y:We.y+Xt,width:R.currentRect.width,height:R.currentRect.height}]])),fo.current?.(ho,Xt)},Ae=()=>{window.removeEventListener("mousemove",at),window.removeEventListener("mouseup",Ae),$e.current=null,dt([]),Ht(new Map);let Ve=Lt().querySelector(`[data-rearrange-section="${R.id}"]`);Ve&&(Ve.style.transform=""),Te&&t({...e,sections:le.map(et=>et.id===R.id?{...et,currentRect:{...et.currentRect,x:Math.max(0,We.x+wt),y:Math.max(0,We.y+ht)}}:et),originalOrder:we}),ze.current?.(wt,ht,Te)};window.addEventListener("mousemove",at),window.addEventListener("mouseup",Ae)}else if(se&&U){D.preventDefault();for(let R of f)try{let le=document.querySelector(R.selector);if(le&&le===U){let we=new Set([R.id]);C(we),po.current?.(we,ce);return}}catch{}ce||(C(new Set),po.current?.(new Set,!1))}else ce||(C(new Set),po.current?.(new Set,!1))};return document.addEventListener("mousedown",L,!0),()=>document.removeEventListener("mousedown",L,!0)},[f,e,t]),(0,ke.useEffect)(()=>{let L=D=>{let z=D.composedPath()[0]||D.target;if(!(z.tagName==="INPUT"||z.tagName==="TEXTAREA"||z.isContentEditable)){if((D.key==="Backspace"||D.key==="Delete")&&w.size>0){D.preventDefault();let U=new Set(w);q(se=>{let ce=new Set(se);for(let R of U)ce.add(R);return ce}),C(new Set),Ne(()=>{let se=k.current;t({...se,sections:se.sections.filter(ce=>!U.has(ce.id)),originalOrder:se.originalOrder.filter(ce=>!U.has(ce))}),q(ce=>{let R=new Set(ce);for(let le of U)R.delete(le);return R})},180);return}if(["ArrowUp","ArrowDown","ArrowLeft","ArrowRight"].includes(D.key)&&w.size>0){D.preventDefault();let U=D.shiftKey?20:1,se=D.key==="ArrowLeft"?-U:D.key==="ArrowRight"?U:0,ce=D.key==="ArrowUp"?-U:D.key==="ArrowDown"?U:0;t({...e,sections:f.map(R=>w.has(R.id)?{...R,currentRect:{...R.currentRect,x:Math.max(0,R.currentRect.x+se),y:Math.max(0,R.currentRect.y+ce)}}:R)});return}D.key==="Escape"&&w.size>0&&C(new Set)}};return document.addEventListener("keydown",L),()=>document.removeEventListener("keydown",L)},[w,f,e,t]);let Lr=(0,ke.useCallback)((L,D)=>{if(L.button!==0)return;let z=L.target;if(z.closest(`.${$.handle}`)||z.closest(`.${$.deleteButton}`))return;L.preventDefault(),L.stopPropagation();let U;L.shiftKey||L.metaKey||L.ctrlKey?(U=new Set(w),U.has(D)?U.delete(D):U.add(D)):w.has(D)?U=new Set(w):U=new Set([D]),C(U),(U.size!==w.size||[...U].some(Te=>!w.has(Te)))&&po.current?.(U,!!(L.shiftKey||L.metaKey||L.ctrlKey));let ce=L.clientX,R=L.clientY,le=new Map;for(let Te of f)U.has(Te.id)&&le.set(Te.id,{x:Te.currentRect.x,y:Te.currentRect.y});$e.current="move";let we=!1,He=0,Je=0,it=new Map;for(let Te of f)if(U.has(Te.id)){let wt=Lt().querySelector(`[data-rearrange-section="${Te.id}"]`);it.set(Te.id,{outlineEl:wt,curW:Te.currentRect.width,curH:Te.currentRect.height})}let We=Te=>{let wt=Te.clientX-ce,ht=Te.clientY-R;if(wt===0&&ht===0)return;we=!0;let at=1/0,Ae=1/0,Ve=-1/0,et=-1/0;for(let[Xt,{curW:Io,curH:ui}]of it){let Rn=le.get(Xt);if(!Rn)continue;let pe=Rn.x+wt,Rr=Rn.y+ht;at=Math.min(at,pe),Ae=Math.min(Ae,Rr),Ve=Math.max(Ve,pe+Io),et=Math.max(et,Rr+ui)}let ft=Wy({x:at,y:Ae,width:Ve-at,height:et-Ae},f,U,s),Wt=wt+ft.dx,sn=ht+ft.dy;He=Wt,Je=sn,dt(ft.guides);for(let[,{outlineEl:Xt}]of it)Xt&&(Xt.style.transform=`translate(${Wt}px, ${sn}px)`);let ho=new Map;for(let[Xt,{curW:Io,curH:ui}]of it){let Rn=le.get(Xt);if(Rn){let pe={x:Math.max(0,Rn.x+Wt),y:Math.max(0,Rn.y+sn),width:Io,height:ui};ho.set(Xt,pe)}}Ht(ho),fo.current?.(Wt,sn)},Ze=Te=>{window.removeEventListener("mousemove",We),window.removeEventListener("mouseup",Ze),$e.current=null,dt([]),Ht(new Map);for(let[,{outlineEl:wt}]of it)wt&&(wt.style.transform="");if(we){let wt=Te.clientX-ce,ht=Te.clientY-R;if(Math.abs(wt)<5&&Math.abs(ht)<5)t({...e,sections:f.map(at=>{let Ae=le.get(at.id);return Ae?{...at,currentRect:{...at.currentRect,x:Ae.x,y:Ae.y}}:at})});else{t({...e,sections:f.map(at=>{let Ae=le.get(at.id);return Ae?{...at,currentRect:{...at.currentRect,x:Math.max(0,Ae.x+He),y:Math.max(0,Ae.y+Je)}}:at})}),ze.current?.(He,Je,!0);return}}ze.current?.(0,0,!1)};window.addEventListener("mousemove",We),window.addEventListener("mouseup",Ze)},[w,f,e,t]),No=(0,ke.useCallback)((L,D,z)=>{L.preventDefault(),L.stopPropagation();let U=f.find(Ze=>Ze.id===D);if(!U)return;C(new Set([D])),$e.current="resize";let se=L.clientX,ce=L.clientY,R={...U.currentRect},le=U.originalRect,we=R.width/R.height,He={...R},Je=Lt().querySelector(`[data-rearrange-section="${D}"]`),it=Ze=>{let Te=Ze.clientX-se,wt=Ze.clientY-ce,ht=R.x,at=R.y,Ae=R.width,Ve=R.height;if(z.includes("e")&&(Ae=Math.max(ou,R.width+Te)),z.includes("w")&&(Ae=Math.max(ou,R.width-Te),ht=R.x+R.width-Ae),z.includes("s")&&(Ve=Math.max(ou,R.height+wt)),z.includes("n")&&(Ve=Math.max(ou,R.height-wt),at=R.y+R.height-Ve),Ze.shiftKey)if(z.length===2){let ft=Math.abs(Ae-R.width),Wt=Math.abs(Ve-R.height);ft>Wt?Ve=Ae/we:Ae=Ve*we,z.includes("w")&&(ht=R.x+R.width-Ae),z.includes("n")&&(at=R.y+R.height-Ve)}else z==="e"||z==="w"?Ve=Ae/we:Ae=Ve*we,z==="w"&&(ht=R.x+R.width-Ae),z==="n"&&(at=R.y+R.height-Ve);He={x:ht,y:at,width:Ae,height:Ve},Je&&(Je.style.left=`${ht}px`,Je.style.top=`${at-ut}px`,Je.style.width=`${Ae}px`,Je.style.height=`${Ve}px`),Me({x:Ze.clientX+12,y:Ze.clientY+12,text:`${Math.round(Ae)} \xD7 ${Math.round(Ve)}`}),Ht(new Map([[D,He]]))},We=()=>{window.removeEventListener("mousemove",it),window.removeEventListener("mouseup",We),Me(null),$e.current=null,Ht(new Map),t({...e,sections:f.map(Ze=>Ze.id===D?{...Ze,currentRect:He}:Ze)})};window.addEventListener("mousemove",it),window.addEventListener("mouseup",We)},[f,e,t,ut]),Ko=(0,ke.useCallback)(L=>{q(D=>{let z=new Set(D);return z.add(L),z}),C(D=>{let z=new Set(D);return z.delete(L),z}),Ne(()=>{let D=k.current;t({...D,sections:D.sections.filter(z=>z.id!==L),originalOrder:D.originalOrder.filter(z=>z!==L)}),q(z=>{let U=new Set(z);return U.delete(L),U})},180)},[t]),G=L=>{let D=L.originalRect,z=L.currentRect;return Math.abs(D.x-z.x)>1||Math.abs(D.y-z.y)>1||Math.abs(D.width-z.width)>1||Math.abs(D.height-z.height)>1},ie=L=>{let D=L.originalRect,z=L.currentRect;return Math.abs(D.x-z.x)>1||Math.abs(D.y-z.y)>1},xe=L=>{let D=L.originalRect,z=L.currentRect;return Math.abs(D.width-z.width)>1||Math.abs(D.height-z.height)>1};for(let L of f)vt.current.has(L.id)||(ie(L)?vt.current.set(L.id,"move"):xe(L)&&vt.current.set(L.id,"resize"));for(let L of vt.current.keys())f.some(D=>D.id===L)||vt.current.delete(L);let De=f.filter(L=>{try{if(K.has(L.id)||w.has(L.id))return!0;let D=document.querySelector(L.selector);if(!D)return!1;let z=D.getBoundingClientRect(),U=L.originalRect;return Math.abs(z.width-U.width)+Math.abs(z.height-U.height)<200}catch{return!1}}),Se=De.filter(L=>G(L)),gt=De.filter(L=>!G(L)),rt=new Set(Se.map(L=>L.id));for(let L of Fe.current)rt.has(L)||Fe.current.delete(L);let Ge=[...rt].sort().join(",");for(let L of Se)qo.current.set(L.id,{currentRect:L.currentRect,originalRect:L.originalRect,isFixed:L.isFixed});(0,ke.useEffect)(()=>{let L=mo.current;mo.current=rt;let D=new Map;for(let z of L)if(!rt.has(z)){if(!f.some(se=>se.id===z))continue;let U=qo.current.get(z);U&&(D.set(z,{orig:U.originalRect,target:U.currentRect,isFixed:U.isFixed}),qo.current.delete(z))}if(D.size>0){ci(U=>{let se=new Map(U);for(let[ce,R]of D)se.set(ce,R);return se});let z=Ne(()=>{ci(U=>{let se=new Map(U);for(let ce of D.keys())se.delete(ce);return se})},250);return()=>clearTimeout(z)}},[Ge,f]);let Ye=(0,ke.useRef)(null),Lt=()=>Ye.current?.getRootNode()??document;return(0,Ke.jsxs)(Ke.Fragment,{children:[(0,Ke.jsxs)("div",{ref:Ye,className:`${$.rearrangeOverlay} ${n?"":$.light} ${o?$.overlayExiting:""}${r?` ${r}`:""}`,"data-feedback-toolbar":!0,children:[Ie&&(0,Ke.jsx)("div",{className:$.hoverHighlight,style:{left:Ie.x,top:Ie.y,width:Ie.w,height:Ie.h}}),gt.map(L=>{let D=L.currentRect,z=L.isFixed?D.y:D.y-ut,U=By,se=w.has(L.id);return(0,Ke.jsxs)("div",{"data-rearrange-section":L.id,className:`${$.sectionOutline} ${se?$.selected:""} ${m||o||K.has(L.id)?$.exiting:""}`,style:{left:D.x,top:z,width:D.width,height:D.height,borderColor:U.border,backgroundColor:U.bg,...en?{}:{opacity:0,animation:"none",transition:"none"}},onMouseDown:ce=>Lr(ce,L.id),onDoubleClick:()=>j(L.id),children:[(0,Ke.jsx)("span",{className:$.sectionLabel,style:{backgroundColor:U.pill},children:L.label}),(0,Ke.jsx)("span",{className:`${$.sectionAnnotation} ${L.note?$.annotationVisible:""}`,children:(L.note&&Z.current.set(L.id,L.note),L.note||Z.current.get(L.id)||"")}),(0,Ke.jsxs)("span",{className:$.sectionDimensions,children:[Math.round(D.width)," \xD7 ",Math.round(D.height)]}),(0,Ke.jsx)("div",{className:$.deleteButton,onMouseDown:ce=>ce.stopPropagation(),onClick:()=>Ko(L.id),children:"\u2715"}),zy.map(ce=>(0,Ke.jsx)("div",{className:`${$.handle} ${$[`handle${ce.charAt(0).toUpperCase()}${ce.slice(1)}`]}`,onMouseDown:R=>No(R,L.id,ce)},ce))]},L.id)}),Se.map(L=>{let D=L.currentRect,z=L.isFixed?D.y:D.y-ut,U=w.has(L.id),se=ie(L),ce=xe(L);if(i&&!U)return null;let le=!Fe.current.has(L.id);return le&&Fe.current.add(L.id),(0,Ke.jsxs)("div",{"data-rearrange-section":L.id,className:`${$.ghostOutline} ${U?$.selected:""} ${m||o||K.has(L.id)?$.exiting:""}`,style:{left:D.x,top:z,width:D.width,height:D.height,...en?{}:{opacity:0,animation:"none",transition:"none"},...le?{}:{animation:"none"}},onMouseDown:we=>Lr(we,L.id),onDoubleClick:()=>j(L.id),children:[(0,Ke.jsx)("span",{className:$.sectionLabel,style:{backgroundColor:By.pill},children:L.label}),(0,Ke.jsx)("span",{className:`${$.sectionAnnotation} ${L.note?$.annotationVisible:""}`,children:(L.note&&Z.current.set(L.id,L.note),L.note||Z.current.get(L.id)||"")}),(0,Ke.jsxs)("span",{className:$.sectionDimensions,children:[Math.round(D.width)," \xD7 ",Math.round(D.height)]}),(0,Ke.jsx)("div",{className:$.deleteButton,onMouseDown:we=>we.stopPropagation(),onClick:()=>Ko(L.id),children:"\u2715"}),zy.map(we=>(0,Ke.jsx)("div",{className:`${$.handle} ${$[`handle${we.charAt(0).toUpperCase()}${we.slice(1)}`]}`,onMouseDown:He=>No(He,L.id,we)},we)),(0,Ke.jsx)("span",{className:$.ghostBadge,children:(()=>{let we=vt.current.get(L.id);if(se&&ce){let[He,Je]=we==="resize"?["Resize","Move"]:["Move","Resize"];return(0,Ke.jsxs)(Ke.Fragment,{children:["Suggested ",He," ",(0,Ke.jsxs)("span",{className:$.ghostBadgeExtra,children:["& ",Je]})]})}return`Suggested ${ce?"Resize":"Move"}`})()})]},L.id)})]}),!i&&(()=>{let L=[];for(let D of Se){let z=Ct.get(D.id);L.push({id:D.id,orig:D.originalRect,target:z||D.currentRect,isFixed:D.isFixed,isSelected:w.has(D.id),isExiting:K.has(D.id)})}for(let[D,z]of Ct)if(!L.some(U=>U.id===D)){let U=f.find(se=>se.id===D);U&&L.push({id:D,orig:U.originalRect,target:z,isFixed:U.isFixed,isSelected:w.has(D)})}for(let[D,z]of Zt)L.some(U=>U.id===D)||L.push({id:D,orig:z.orig,target:z.target,isFixed:z.isFixed,isSelected:!1,isExiting:!0});return L.length===0?null:(0,Ke.jsxs)("svg",{className:`${$.connectorSvg} ${m||o?$.connectorExiting:""}`,children:[L.map(({id:D,orig:z,target:U,isFixed:se,isSelected:ce,isExiting:R})=>{let le=z.x+z.width/2,we=(se?z.y:z.y-ut)+z.height/2,He=U.x+U.width/2,Je=(se?U.y:U.y-ut)+U.height/2,it=He-le,We=Je-we,Ze=Math.sqrt(it*it+We*We);if(Ze<2)return null;let Te=Math.min(1,Ze/40),wt=Math.min(Ze*.3,60),ht=Ze>0?-We/Ze:0,at=Ze>0?it/Ze:0,Ae=(le+He)/2+ht*wt,Ve=(we+Je)/2+at*wt,et=Ct.has(D),ft=et||ce?1:.4,Wt=et||ce?1:.5;return(0,Ke.jsxs)("g",{className:R?$.connectorExiting:"",children:[(0,Ke.jsx)("path",{className:$.connectorLine,d:`M ${le} ${we} Q ${Ae} ${Ve} ${He} ${Je}`,fill:"none",stroke:"rgba(59, 130, 246, 0.45)",strokeWidth:"1.5",opacity:ft*Te}),(0,Ke.jsx)("circle",{className:$.connectorDot,cx:le,cy:we,r:4*Te,fill:"rgba(59, 130, 246, 0.8)",stroke:"#fff",strokeWidth:"1.5",opacity:Wt*Te,filter:"url(#connDotShadow)"}),(0,Ke.jsx)("circle",{className:$.connectorDot,cx:He,cy:Je,r:4*Te,fill:"rgba(59, 130, 246, 0.8)",stroke:"#fff",strokeWidth:"1.5",opacity:Wt*Te,filter:"url(#connDotShadow)"})]},`conn-${D}`)}),(0,Ke.jsx)("defs",{children:(0,Ke.jsx)("filter",{id:"connDotShadow",x:"-50%",y:"-50%",width:"200%",height:"200%",children:(0,Ke.jsx)("feDropShadow",{dx:"0",dy:"0.5",stdDeviation:"1",floodOpacity:"0.15"})})})]})})(),g&&(()=>{let L=f.find(Je=>Je.id===g);if(!L)return null;let D=L.currentRect,z=L.isFixed?D.y:D.y-ut,U=D.x+D.width/2,se=z-8,ce=z+D.height+8,R=se>200,le=ce<window.innerHeight-100,we=Math.max(160,Math.min(window.innerWidth-160,U)),He;return R?He={left:we,bottom:window.innerHeight-se}:le?He={left:we,top:ce}:He={left:we,top:Math.max(80,window.innerHeight/2-80)},(0,Ke.jsx)(cs,{element:L.label,placeholder:"Add a note about this section",initialValue:L.note??"",submitLabel:O.current?"Save":"Set",onSubmit:Y,onCancel:A,onDelete:O.current?()=>{Y("")}:void 0,isExiting:h,lightMode:!n,style:He})})(),me&&(0,Ke.jsx)("div",{className:$.sizeIndicator,style:{left:me.x,top:me.y},"data-feedback-toolbar":!0,children:me.text}),B.map((L,D)=>(0,Ke.jsx)("div",{className:$.guideLine,style:L.axis==="x"?{position:"fixed",left:L.pos,top:0,width:1,height:"100vh"}:{position:"fixed",left:0,top:L.pos-ut,width:"100vw",height:1}},`${L.axis}-${L.pos}-${D}`))]})}var Rm=new Set(["script","style","noscript","link","meta","br","hr"]);function Xw(){let e=document.querySelector("main")||document.body,t=[],n=Array.from(e.children),o=e!==document.body&&n.length<3?Array.from(document.body.children):n;for(let r of o){if(!(r instanceof HTMLElement)||Rm.has(r.tagName.toLowerCase())||r.hasAttribute("data-feedback-toolbar"))continue;let i=window.getComputedStyle(r);if(i.display==="none"||i.visibility==="hidden")continue;let s=r.getBoundingClientRect();if(!(s.height<10||s.width<10)){t.push({label:Fl(r),selector:Cr(r),top:s.top,bottom:s.bottom,left:s.left,right:s.right,area:s.width*s.height});for(let l of Array.from(r.children)){if(!(l instanceof HTMLElement)||Rm.has(l.tagName.toLowerCase())||l.hasAttribute("data-feedback-toolbar"))continue;let a=window.getComputedStyle(l);if(a.display==="none"||a.visibility==="hidden")continue;let c=l.getBoundingClientRect();c.height<10||c.width<10||t.push({label:Fl(l),selector:Cr(l),top:c.top,bottom:c.bottom,left:c.left,right:c.right,area:c.width*c.height})}}}return t}function Qw(e){let t=window.scrollY;return e.map(({label:n,selector:o,rect:r})=>{let i=r.y-t;return{label:n,selector:o,top:i,bottom:i+r.height,left:r.x,right:r.x+r.width,area:r.width*r.height}})}function Gw(e){let t=window.scrollY,n=e.y-t,o=e.x;return{top:n,bottom:n+e.height,left:o,right:o+e.width,area:e.width*e.height}}function iu(e,t){let n=t?Qw(t):Xw(),o=Gw(e),r=null,i=null,s=null,l=null,a=null;for(let w of n){if(Math.abs(w.left-o.left)<2&&Math.abs(w.top-o.top)<2&&Math.abs(w.right-w.left-e.width)<2&&Math.abs(w.bottom-w.top-e.height)<2)continue;w.left<=o.left+2&&w.right>=o.right-2&&w.top<=o.top+2&&w.bottom>=o.bottom-2&&w.area>o.area*1.5&&(!a||w.area<a._area)&&(a={label:w.label,selector:w.selector,_area:w.area});let C=o.right>w.left+5&&o.left<w.right-5,S=o.bottom>w.top+5&&o.top<w.bottom-5;if(C&&w.bottom<=o.top+5){let g=Math.round(o.top-w.bottom);(!r||g<r._dist)&&(r={label:w.label,selector:w.selector,gap:Math.max(0,g),_dist:g})}if(C&&w.top>=o.bottom-5){let g=Math.round(w.top-o.bottom);(!i||g<i._dist)&&(i={label:w.label,selector:w.selector,gap:Math.max(0,g),_dist:g})}if(S&&w.right<=o.left+5){let g=Math.round(o.left-w.right);(!s||g<s._dist)&&(s={label:w.label,selector:w.selector,gap:Math.max(0,g),_dist:g})}if(S&&w.left>=o.right-5){let g=Math.round(w.left-o.right);(!l||g<l._dist)&&(l={label:w.label,selector:w.selector,gap:Math.max(0,g),_dist:g})}}let c=window.innerWidth,d=window.innerHeight,m=Kw(e,c),f=w=>w?{label:w.label,selector:w.selector,gap:w.gap}:null,k=qw(o,e,c,d,a?{label:a.label,selector:a.selector,_area:a._area}:null,n);return{above:f(r),below:f(i),left:f(s),right:f(l),alignment:m,containedIn:a?{label:a.label,selector:a.selector}:null,outOfBounds:k}}function qw(e,t,n,o,r,i){let s={},l=!1,a=[];if(e.left<-2&&a.push("left"),e.right>n+2&&a.push("right"),e.top<-2&&a.push("top"),e.bottom>o+2&&a.push("bottom"),a.length>0&&(s.viewport=a,l=!0),r){let c=i.find(d=>d.label===r.label&&d.selector===r.selector&&Math.abs(d.area-r._area)<10);if(c){let d=[];e.left<c.left-2&&d.push("left"),e.right>c.right+2&&d.push("right"),e.top<c.top-2&&d.push("top"),e.bottom>c.bottom+2&&d.push("bottom"),d.length>0&&(s.container={label:r.label,edges:d},l=!0)}}return l?s:null}function Kw(e,t){if(e.width/t>.85)return"full-width";let o=e.x+e.width/2,r=t/2,i=o-r,s=t*.08;return Math.abs(i)<s?"center":i<0?"left":"right"}function Xy(e){switch(e){case"full-width":return"full-width";case"center":return"centered";case"left":return"left-aligned";case"right":return"right-aligned"}}function Pm(e,t={}){let n=[];e.above&&n.push(`Below \`${e.above.label}\`${e.above.gap>0?` (${e.above.gap}px gap)`:""}`),e.below&&n.push(`Above \`${e.below.label}\`${e.below.gap>0?` (${e.below.gap}px gap)`:""}`),t.includeLeftRight&&(e.left&&n.push(`Right of \`${e.left.label}\`${e.left.gap>0?` (${e.left.gap}px gap)`:""}`),e.right&&n.push(`Left of \`${e.right.label}\`${e.right.gap>0?` (${e.right.gap}px gap)`:""}`));let o=Xy(e.alignment);return e.containedIn?n.push(`${o.charAt(0).toUpperCase()+o.slice(1)} in \`${e.containedIn.label}\``):n.push(`${o.charAt(0).toUpperCase()+o.slice(1)} in page`),t.includePixelRef&&t.pixelRef&&n.push(`Pixel ref: \`${t.pixelRef}\``),e.outOfBounds&&(e.outOfBounds.viewport&&n.push(`**Outside viewport** (${e.outOfBounds.viewport.join(", ")} edge${e.outOfBounds.viewport.length>1?"s":""})`),e.outOfBounds.container&&n.push(`**Outside \`${e.outOfBounds.container.label}\`** (${e.outOfBounds.container.edges.join(", ")} edge${e.outOfBounds.container.edges.length>1?"s":""})`)),n}function Qy(e,t,n){let o=[];e.above&&o.push(`below \`${e.above.label}\``),e.below&&o.push(`above \`${e.below.label}\``),e.left&&o.push(`right of \`${e.left.label}\``),e.right&&o.push(`left of \`${e.right.label}\``),e.containedIn&&o.push(`inside \`${e.containedIn.label}\``),o.push(Xy(e.alignment)),e.outOfBounds?.viewport&&o.push(`**outside viewport** (${e.outOfBounds.viewport.join(", ")})`),e.outOfBounds?.container&&o.push(`**outside \`${e.outOfBounds.container.label}\`** (${e.outOfBounds.container.edges.join(", ")})`);let r=n?`, ${Math.round(n.width)}\xD7${Math.round(n.height)}px`:"";return`at (${Math.round(t.x)}, ${Math.round(t.y)})${r}: ${o.join(", ")}`}var Yy=15;function Vy(e){if(e.length<2)return[];let t=[],n=new Set;for(let o=0;o<e.length;o++){if(n.has(o))continue;let r=[o];for(let i=o+1;i<e.length;i++)n.has(i)||Math.abs(e[o].rect.y-e[i].rect.y)<Yy&&r.push(i);if(r.length>=2){let i=r.map(a=>e[a]);i.sort((a,c)=>a.rect.x-c.rect.x);let s=[];for(let a=0;a<i.length-1;a++)s.push(Math.round(i[a+1].rect.x-(i[a].rect.x+i[a].rect.width)));let l=Math.round(i.reduce((a,c)=>a+c.rect.y,0)/i.length);t.push({labels:i.map(a=>a.label),type:"row",sharedEdge:l,gaps:s,avgGap:s.length?Math.round(s.reduce((a,c)=>a+c,0)/s.length):0}),r.forEach(a=>n.add(a))}}for(let o=0;o<e.length;o++){if(n.has(o))continue;let r=[o];for(let i=o+1;i<e.length;i++)n.has(i)||Math.abs(e[o].rect.x-e[i].rect.x)<Yy&&r.push(i);if(r.length>=2){let i=r.map(a=>e[a]);i.sort((a,c)=>a.rect.y-c.rect.y);let s=[];for(let a=0;a<i.length-1;a++)s.push(Math.round(i[a+1].rect.y-(i[a].rect.y+i[a].rect.height)));let l=Math.round(i.reduce((a,c)=>a+c.rect.x,0)/i.length);t.push({labels:i.map(a=>a.label),type:"column",sharedEdge:l,gaps:s,avgGap:s.length?Math.round(s.reduce((a,c)=>a+c,0)/s.length):0}),r.forEach(a=>n.add(a))}}return t}function Gy(e){if(e.length<2)return[];let t=Vy(e.map(s=>({label:s.label,rect:s.originalRect}))),n=Vy(e.map(s=>({label:s.label,rect:s.currentRect}))),o=[],r=new Set;for(let s of t){let l=new Set(s.labels),a=null,c=0;for(let d of n){let m=d.labels.filter(f=>l.has(f)).length;m>=2&&m>c&&(a=d,c=m)}if(a){let d=a.labels.filter(f=>l.has(f)),m=d.join(", ");if(a.type!==s.type){let f=s.type==="row"?"y":"x",k=a.type==="row"?"y":"x";o.push(`**${m}**: ${s.type} (${f}\u2248${s.sharedEdge}, ${s.avgGap}px gaps) \u2192 ${a.type} (${k}\u2248${a.sharedEdge}, ${a.avgGap}px gaps)`)}else if(Math.abs(s.sharedEdge-a.sharedEdge)>20||Math.abs(s.avgGap-a.avgGap)>5){let f=s.type==="row"?"y":"x",k=Math.abs(s.sharedEdge-a.sharedEdge)>20?` ${f}: ${s.sharedEdge} \u2192 ${a.sharedEdge}`:"",w=Math.abs(s.avgGap-a.avgGap)>5?` gaps: ${s.avgGap}px \u2192 ${a.avgGap}px`:"";o.push(`**${m}**: ${s.type} shifted \u2014${k}${w}`)}d.forEach(f=>r.add(f))}else{let d=s.labels.join(", "),m=s.type==="row"?"y":"x";o.push(`**${d}**: ${s.type} (${m}\u2248${s.sharedEdge}) dissolved`),s.labels.forEach(f=>r.add(f))}}for(let s of n){if(s.labels.every(c=>r.has(c))||s.labels.filter(c=>!r.has(c)).length<2)continue;if(!t.some(c=>c.labels.filter(m=>s.labels.includes(m)).length>=2)){let c=s.type==="row"?"y":"x";o.push(`**${s.labels.join(", ")}**: new ${s.type} (${c}\u2248${s.sharedEdge}, ${s.avgGap}px gaps)`),s.labels.forEach(d=>r.add(d))}}let i=e.filter(s=>!r.has(s.label));if(i.length>=2){let s={};for(let l of i){let a=Math.round(l.currentRect.x/5)*5;(s[a]??(s[a]=[])).push(l.label)}for(let[l,a]of Object.entries(s))a.length>=2&&o.push(`**${a.join(", ")}**: shared left edge at x\u2248${l}`)}return o}function Nm(e){if(typeof document>"u")return{viewport:e,contentArea:null};let t=[],n=new Set,o=l=>{n.has(l)||l instanceof HTMLElement&&(l.hasAttribute("data-feedback-toolbar")||Rm.has(l.tagName.toLowerCase())||(n.add(l),t.push(l)))},r=document.querySelector("main");r&&o(r);let i=document.querySelector("[role='main']");i&&o(i);for(let l of Array.from(document.body.children))if(o(l),l.children){for(let a of Array.from(l.children))if(o(a),a.children)for(let c of Array.from(a.children))o(c)}let s=null;for(let l of t){let a=l.getBoundingClientRect();if(a.height<50)continue;let c=getComputedStyle(l);if(c.maxWidth&&c.maxWidth!=="none"&&c.maxWidth!=="0px"){(!s||a.width<s.rect.width)&&(s={el:l,rect:a});continue}!s&&a.width<e.width-20&&a.width>100&&(s={el:l,rect:a})}if(s){let{el:l,rect:a}=s;return{viewport:e,contentArea:{width:Math.round(a.width),left:Math.round(a.left),right:Math.round(a.right),centerX:Math.round(a.left+a.width/2),selector:Cr(l)}}}return{viewport:e,contentArea:null}}function qy(e){if(typeof document>"u")return null;let t=document.querySelector(e);if(!t?.parentElement)return null;let n=getComputedStyle(t.parentElement),o={parentDisplay:n.display,parentSelector:Cr(t.parentElement)};return n.display.includes("flex")&&(o.flexDirection=n.flexDirection),n.display.includes("grid")&&n.gridTemplateColumns!=="none"&&(o.gridCols=n.gridTemplateColumns),n.gap&&n.gap!=="normal"&&n.gap!=="0px"&&(o.gap=n.gap),o}function Im(e,t){let n=t.contentArea,o=n?n.width:t.viewport.width,r=n?n.left:0,i=n?n.centerX:Math.round(t.viewport.width/2),s=Math.round(e.x-r),l=Math.round(r+o-(e.x+e.width)),a=(e.width/o*100).toFixed(1),c=e.x+e.width/2,d=Math.abs(c-i)<20,m=e.width/o>.95,f=[];return m?f.push("`width: 100%` of container"):f.push(`left \`${s}px\` in container, right \`${l}px\`, width \`${a}%\` (\`${Math.round(e.width)}px\`)`),d&&!m&&f.push("centered \u2014 `margin-inline: auto`"),f.join(" \u2014 ")}function Ky(e){let{viewport:t,contentArea:n}=e,o=`### Reference Frame
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
`,o}function Jw(e){let t=qy(e);if(!t)return null;let n=`\`${t.parentDisplay}\``;return t.flexDirection&&(n+=`, flex-direction: \`${t.flexDirection}\``),t.gridCols&&(n+=`, grid-template-columns: \`${t.gridCols}\``),t.gap&&(n+=`, gap: \`${t.gap}\``),`Parent: ${n} (\`${t.parentSelector}\`)`}function Dm(e,t,n,o="standard"){if(e.length===0)return"";let r=[...e].sort((S,g)=>Math.abs(S.y-g.y)<20?S.x-g.x:S.y-g.y),i="";if(n?.blankCanvas?(i+=`## Wireframe: New Page

`,n.wireframePurpose&&(i+=`> **Purpose:** ${n.wireframePurpose}
>
`),i+=`> ${e.length} component${e.length!==1?"s":""} placed \u2014 this is a standalone wireframe, not related to the current page.
>
> This wireframe is a rough sketch for exploring ideas.

`):i+=`## Design Layout

> ${e.length} component${e.length!==1?"s":""} placed

`,o==="compact")return i+=`### Components
`,r.forEach((S,g)=>{let x=Hn[S.type]?.label||S.type;i+=`${g+1}. **${x}** \u2014 \`${Math.round(S.width)}\xD7${Math.round(S.height)}px\` at \`(${Math.round(S.x)}, ${Math.round(S.y)})\`
`,S.text&&(i+=`   - Note: "${S.text}"
`)}),i;let s=Nm(t);i+=Ky(s),i+=`### Components
`,r.forEach((S,g)=>{let x=Hn[S.type]?.label||S.type,h={x:S.x,y:S.y,width:S.width,height:S.height};i+=`${g+1}. **${x}** \u2014 \`${Math.round(S.width)}\xD7${Math.round(S.height)}px\` at \`(${Math.round(S.x)}, ${Math.round(S.y)})\`
`,S.text&&(i+=`   - Note: "${S.text}"
`);let E=iu(h),j=Pm(E,{includeLeftRight:o==="detailed"||o==="forensic"});for(let Y of j)i+=`   - ${Y}
`;let A=Im(h,s);A&&(i+=`   - CSS: ${A}
`)}),i+=`
### Layout Analysis
`;let l=[];for(let S of r){let g=l.find(x=>Math.abs(x.y-S.y)<30);g?g.items.push(S):l.push({y:S.y,items:[S]})}if(l.sort((S,g)=>S.y-g.y),l.forEach((S,g)=>{S.items.sort((h,E)=>h.x-E.x);let x=S.items.map(h=>Hn[h.type]?.label||h.type);if(S.items.length===1){let E=S.items[0].width>t.width*.8;i+=`- Row ${g+1} (y\u2248${Math.round(S.y)}): ${x[0]}${E?" \u2014 full width":""}
`}else i+=`- Row ${g+1} (y\u2248${Math.round(S.y)}): ${x.join(" | ")} \u2014 ${S.items.length} items side by side
`}),o==="detailed"||o==="forensic"){i+=`
### Spacing & Gaps
`;for(let S=0;S<r.length-1;S++){let g=r[S],x=r[S+1],h=Hn[g.type]?.label||g.type,E=Hn[x.type]?.label||x.type,O=Math.round(x.y-(g.y+g.height)),j=Math.round(x.x-(g.x+g.width));Math.abs(g.y-x.y)<30?i+=`- ${h} \u2192 ${E}: \`${j}px\` horizontal gap
`:i+=`- ${h} \u2192 ${E}: \`${O}px\` vertical gap
`}if(o==="forensic"&&r.length>2){i+=`
### All Pairwise Gaps
`;for(let S=0;S<r.length;S++)for(let g=S+1;g<r.length;g++){let x=r[S],h=r[g],E=Hn[x.type]?.label||x.type,O=Hn[h.type]?.label||h.type,j=Math.round(h.y-(x.y+x.height)),A=Math.round(h.x-(x.x+x.width));i+=`- ${E} \u2194 ${O}: h=\`${A}px\` v=\`${j}px\`
`}}o==="forensic"&&(i+=`
### Z-Order (placement order)
`,e.forEach((S,g)=>{let x=Hn[S.type]?.label||S.type;i+=`${g}. ${x} at \`(${Math.round(S.x)}, ${Math.round(S.y)})\`
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
`}a&&(i+="- Navigation: `position: sticky; top: 0; z-index: 50;`\n")}return i}function Am(e,t="standard",n){let{sections:o}=e,r=[];for(let d of o){let m=d.originalRect,f=d.currentRect,k=Math.abs(m.x-f.x)>1||Math.abs(m.y-f.y)>1,w=Math.abs(m.width-f.width)>1||Math.abs(m.height-f.height)>1,C=!!d.note;if(!k&&!w&&!C){t==="forensic"&&r.push({section:d,posMoved:!1,sizeChanged:!1});continue}r.push({section:d,posMoved:k,sizeChanged:w})}if(r.length===0||t!=="forensic"&&r.every(d=>!d.posMoved&&!d.sizeChanged&&!d.section.note))return"";let i=`## Suggested Layout Changes

`,s=n?n.width:typeof window<"u"?window.innerWidth:0,l=n?n.height:typeof window<"u"?window.innerHeight:0,a=Nm({width:s,height:l});t!=="compact"&&(i+=Ky(a)),t==="forensic"&&(i+=`> Detected at: \`${new Date(e.detectedAt).toISOString()}\`
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
`),m){let S=iu(k,c("original")),g=iu(w,c("current")),x=f?{width:k.width,height:k.height}:void 0;i+=`  - Currently ${Qy(S,{x:k.x,y:k.y},x)}
`;let h=f?{width:w.width,height:w.height}:void 0,E=`at (${Math.round(w.x)}, ${Math.round(w.y)})`,O=h?`, ${Math.round(h.width)}\xD7${Math.round(h.height)}px`:"",A=Pm(g,{includeLeftRight:t==="detailed"||t==="forensic"});if(A.length>0){i+=`  - Suggested position ${E}${O}: ${A[0]}
`;for(let K=1;K<A.length;K++)i+=`    ${A[K]}
`}else i+=`  - Suggested position ${E}${O}
`;let Y=Im(w,a);Y&&(i+=`  - CSS: ${Y}
`)}let C=Jw(d.selector);if(C&&(i+=`  - ${C}
`),i+=`  - Selector: \`${d.selector}\`
`,t==="detailed"||t==="forensic"){let S=d.className?`${d.tagName}.${d.className.split(" ")[0]}`:d.tagName;S!==d.selector&&(i+=`  - Element: \`${S}\`
`),d.role&&(i+=`  - Role: \`${d.role}\`
`),t==="forensic"&&d.textSnippet&&(i+=`  - Text: "${d.textSnippet}"
`)}t==="forensic"&&(i+=`  - Original rect: \`{ x: ${Math.round(k.x)}, y: ${Math.round(k.y)}, w: ${Math.round(k.width)}, h: ${Math.round(k.height)} }\`
`,i+=`  - Current rect: \`{ x: ${Math.round(w.x)}, y: ${Math.round(w.y)}, w: ${Math.round(w.width)}, h: ${Math.round(w.height)} }\`
`)}if(t!=="compact"){let d=r.filter(f=>f.posMoved).map(f=>({label:f.section.label,originalRect:f.section.originalRect,currentRect:f.section.currentRect})),m=Gy(d);if(m.length>0){i+=`
### Layout Summary
`;for(let f of m)i+=`- ${f}
`}}if(t!=="compact"&&o.length>1){i+=`
### All Sections (current positions)
`;let d=[...o].sort((m,f)=>Math.abs(m.currentRect.y-f.currentRect.y)<20?m.currentRect.x-f.currentRect.x:m.currentRect.y-f.currentRect.y);for(let m of d){let f=m.currentRect,k=Math.abs(f.x-m.originalRect.x)>1||Math.abs(f.y-m.originalRect.y)>1||Math.abs(f.width-m.originalRect.width)>1||Math.abs(f.height-m.originalRect.height)>1;i+=`- ${m.label}: \`${Math.round(f.width)}\xD7${Math.round(f.height)}px\` at \`(${Math.round(f.x)}, ${Math.round(f.y)})\`${k?" \u2190 suggested":""}
`}}return i}var $m="feedback-annotations-";function Ol(e){return`${$m}${e}`}function Go(e){if(typeof window>"u")return[];try{let t=localStorage.getItem(Ol(e));if(!t)return[];let n=JSON.parse(t),o=Date.now()-10080*60*1e3;return n.filter(r=>!r.timestamp||r.timestamp>o)}catch{return[]}}function Om(e,t){if(!(typeof window>"u"))try{localStorage.setItem(Ol(e),JSON.stringify(t))}catch{try{localStorage.setItem(Ol(e),JSON.stringify(Zw(t)))}catch{}}}function Zw(e){return e.map(t=>{if(!t||typeof t!="object"||!("screenshot"in t))return t;let{screenshot:n,...o}=t;return o})}function Jy(){let e=new Map;if(typeof window>"u")return e;try{let t=Date.now()-6048e5;for(let n=0;n<localStorage.length;n++){let o=localStorage.key(n);if(o?.startsWith($m)){let r=o.slice($m.length),i=localStorage.getItem(o);if(i){let l=JSON.parse(i).filter(a=>!a.timestamp||a.timestamp>t);l.length>0&&e.set(r,l)}}}}catch{}return e}function su(e,t,n){let o=t.map(r=>({...r,_syncedTo:n}));Om(e,o)}var Bm="agentation-design-";function Zy(e){if(typeof window>"u")return[];try{let t=localStorage.getItem(`${Bm}${e}`);return t?JSON.parse(t):[]}catch{return[]}}function e1(e,t){if(!(typeof window>"u"))try{localStorage.setItem(`${Bm}${e}`,JSON.stringify(t))}catch{}}function t1(e){if(!(typeof window>"u"))try{localStorage.removeItem(`${Bm}${e}`)}catch{}}var zm="agentation-rearrange-";function n1(e){if(typeof window>"u")return null;try{let t=localStorage.getItem(`${zm}${e}`);return t?JSON.parse(t):null}catch{return null}}function o1(e,t){if(!(typeof window>"u"))try{localStorage.setItem(`${zm}${e}`,JSON.stringify(t))}catch{}}function r1(e){if(!(typeof window>"u"))try{localStorage.removeItem(`${zm}${e}`)}catch{}}var Hm="agentation-wireframe-";function i1(e){if(typeof window>"u")return null;try{let t=localStorage.getItem(`${Hm}${e}`);return t?JSON.parse(t):null}catch{return null}}function Wm(e,t){if(!(typeof window>"u"))try{localStorage.setItem(`${Hm}${e}`,JSON.stringify(t))}catch{}}function Bl(e){if(!(typeof window>"u"))try{localStorage.removeItem(`${Hm}${e}`)}catch{}}var s1="agentation-session-";function Um(e){return`${s1}${e}`}function l1(e){if(typeof window>"u")return null;try{return localStorage.getItem(Um(e))}catch{return null}}function lu(e,t){if(!(typeof window>"u"))try{localStorage.setItem(Um(e),t)}catch{}}function a1(e){if(!(typeof window>"u"))try{localStorage.removeItem(Um(e))}catch{}}var Fm=`${s1}toolbar-hidden`;function c1(){if(typeof window>"u")return!1;try{return sessionStorage.getItem(Fm)==="1"}catch{return!1}}function u1(e){if(!(typeof window>"u"))try{e?sessionStorage.setItem(Fm,"1"):sessionStorage.removeItem(Fm)}catch{}}async function au(e,t){let n=await fetch(`${e}/sessions`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({url:t})});if(!n.ok)throw new Error(`Failed to create session: ${n.status}`);return n.json()}async function jm(e,t){let n=await fetch(`${e}/sessions/${t}`);if(!n.ok)throw new Error(`Failed to get session: ${n.status}`);return n.json()}async function Ym(e,t,n){!n.elementPath&&(n.element==="body"||n.element==="html")&&(n={...n,elementPath:n.element});let o=await fetch(`${e}/sessions/${t}/annotations`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(n)});if(!o.ok)throw new Error(`Failed to sync annotation: ${o.status}`);return o.json()}async function cu(e,t,n){let o=await fetch(`${e}/annotations/${t}`,{method:"PATCH",headers:{"Content-Type":"application/json"},body:JSON.stringify(n)});if(!o.ok)throw new Error(`Failed to update annotation: ${o.status}`);return o.json()}async function zl(e,t){let n=await fetch(`${e}/annotations/${t}`,{method:"DELETE"});if(!n.ok)throw new Error(`Failed to delete annotation: ${n.status}`)}var ct={FunctionComponent:0,ClassComponent:1,IndeterminateComponent:2,HostRoot:3,HostPortal:4,HostComponent:5,HostText:6,Fragment:7,Mode:8,ContextConsumer:9,ContextProvider:10,ForwardRef:11,Profiler:12,SuspenseComponent:13,MemoComponent:14,SimpleMemoComponent:15,LazyComponent:16,IncompleteClassComponent:17,DehydratedFragment:18,SuspenseListComponent:19,ScopeComponent:21,OffscreenComponent:22,LegacyHiddenComponent:23,CacheComponent:24,TracingMarkerComponent:25,HostHoistable:26,HostSingleton:27,IncompleteFunctionComponent:28,Throw:29,ViewTransitionComponent:30,ActivityComponent:31},d1=new Set(["Component","PureComponent","Fragment","Suspense","Profiler","StrictMode","Routes","Route","Outlet","Root","ErrorBoundaryHandler","HotReload","Hot"]),_1=[/Boundary$/,/BoundaryHandler$/,/Provider$/,/Consumer$/,/^(Inner|Outer)/,/Router$/,/^Client(Page|Segment|Root)/,/^Segment(ViewNode|Node)$/,/^LayoutSegment/,/^Server(Root|Component|Render)/,/^RSC/,/Context$/,/^Hot(Reload)?$/,/^(Dev|React)(Overlay|Tools|Root)/,/Overlay$/,/Handler$/,/^With[A-Z]/,/Wrapper$/,/^Root$/],eb=[/Page$/,/View$/,/Screen$/,/Section$/,/Card$/,/List$/,/Item$/,/Form$/,/Modal$/,/Dialog$/,/Button$/,/Nav$/,/Header$/,/Footer$/,/Layout$/,/Panel$/,/Tab$/,/Menu$/];function tb(e){let t=e?.mode??"filtered",n=d1;if(e?.skipExact){let o=e.skipExact instanceof Set?e.skipExact:new Set(e.skipExact);n=new Set([...d1,...o])}return{maxComponents:e?.maxComponents??6,maxDepth:e?.maxDepth??30,mode:t,skipExact:n,skipPatterns:e?.skipPatterns?[..._1,...e.skipPatterns]:_1,userPatterns:e?.userPatterns??eb,filter:e?.filter}}function nb(e){return e.replace(/([a-z])([A-Z])/g,"$1-$2").replace(/([A-Z])([A-Z][a-z])/g,"$1-$2").toLowerCase()}function ob(e,t=10){let n=new Set,o=e,r=0;for(;o&&r<t;)o.className&&typeof o.className=="string"&&o.className.split(/\s+/).forEach(i=>{if(i.length>1){let s=i.replace(/[_][a-zA-Z0-9]{5,}.*$/,"").toLowerCase();s.length>1&&n.add(s)}}),o=o.parentElement,r++;return n}function rb(e,t){let n=nb(e);for(let o of t){if(o===n)return!0;let r=n.split("-").filter(s=>s.length>2),i=o.split("-").filter(s=>s.length>2);for(let s of r)for(let l of i)if(s===l||s.includes(l)||l.includes(s))return!0}return!1}function ib(e,t,n,o){if(n.filter)return n.filter(e,t);switch(n.mode){case"all":return!0;case"filtered":return!(n.skipExact.has(e)||n.skipPatterns.some(r=>r.test(e)));case"smart":return n.skipExact.has(e)||n.skipPatterns.some(r=>r.test(e))?!1:!!(o&&rb(e,o)||n.userPatterns.some(r=>r.test(e)));default:return!0}}var _s=null,sb=new WeakMap;function Vm(e){return Object.keys(e).some(t=>t.startsWith("__reactFiber$")||t.startsWith("__reactInternalInstance$")||t.startsWith("__reactProps$"))}function lb(){if(_s!==null)return _s;if(typeof document>"u")return!1;if(document.body&&Vm(document.body))return _s=!0,!0;let e=["#root","#app","#__next","[data-reactroot]"];for(let t of e){let n=document.querySelector(t);if(n&&Vm(n))return _s=!0,!0}if(document.body){for(let t of document.body.children)if(Vm(t))return _s=!0,!0}return _s=!1,!1}var Hl={map:sb};function ab(e){return Object.keys(e).find(n=>n.startsWith("__reactFiber$")||n.startsWith("__reactInternalInstance$"))||null}function cb(e){let t=ab(e);return t?e[t]:null}function ai(e){return e?e.displayName?e.displayName:e.name?e.name:null:null}function ub(e){let{tag:t,type:n,elementType:o}=e;if(t===ct.HostComponent||t===ct.HostText||t===ct.HostHoistable||t===ct.HostSingleton||t===ct.Fragment||t===ct.Mode||t===ct.Profiler||t===ct.DehydratedFragment||t===ct.HostRoot||t===ct.HostPortal||t===ct.ScopeComponent||t===ct.OffscreenComponent||t===ct.LegacyHiddenComponent||t===ct.CacheComponent||t===ct.TracingMarkerComponent||t===ct.Throw||t===ct.ViewTransitionComponent||t===ct.ActivityComponent)return null;if(t===ct.ForwardRef){let r=o;if(r?.render){let i=ai(r.render);if(i)return i}return r?.displayName?r.displayName:ai(n)}if(t===ct.MemoComponent||t===ct.SimpleMemoComponent){let r=o;if(r?.type){let i=ai(r.type);if(i)return i}return r?.displayName?r.displayName:ai(n)}if(t===ct.ContextProvider){let r=n;return r?._context?.displayName?`${r._context.displayName}.Provider`:null}if(t===ct.ContextConsumer){let r=n;return r?.displayName?`${r.displayName}.Consumer`:null}if(t===ct.LazyComponent){let r=o;return r?._status===1&&r._result?ai(r._result):null}return t===ct.SuspenseComponent||t===ct.SuspenseListComponent?null:t===ct.IncompleteClassComponent||t===ct.IncompleteFunctionComponent||t===ct.FunctionComponent||t===ct.ClassComponent||t===ct.IndeterminateComponent?ai(n):null}function db(e){return e.length<=2||e.length<=3&&e===e.toLowerCase()}function m1(e,t){let n=tb(t),o=n.mode==="all";if(o){let a=Hl.map.get(e);if(a!==void 0)return a}if(!lb()){let a={path:null,components:[]};return o&&Hl.map.set(e,a),a}let r=n.mode==="smart"?ob(e):void 0,i=[];try{let a=cb(e),c=0;for(;a&&c<n.maxDepth&&i.length<n.maxComponents;){let d=ub(a);d&&!db(d)&&ib(d,c,n,r)&&i.push(d),a=a.return,c++}}catch{let a={path:null,components:[]};return o&&Hl.map.set(e,a),a}if(i.length===0){let a={path:null,components:[]};return o&&Hl.map.set(e,a),a}let l={path:i.slice().reverse().map(a=>`<${a}>`).join(" "),components:i};return o&&Hl.map.set(e,l),l}var _b=ae(bt()),Wl={FunctionComponent:0,ClassComponent:1,IndeterminateComponent:2,HostRoot:3,HostPortal:4,HostComponent:5,HostText:6,Fragment:7,Mode:8,ContextConsumer:9,ContextProvider:10,ForwardRef:11,Profiler:12,SuspenseComponent:13,MemoComponent:14,SimpleMemoComponent:15,LazyComponent:16};function mb(e){if(!e||typeof e!="object")return null;let t=Object.keys(e),n=t.find(i=>i.startsWith("__reactFiber$"));if(n)return e[n]||null;let o=t.find(i=>i.startsWith("__reactInternalInstance$"));if(o)return e[o]||null;let r=t.find(i=>{if(!i.startsWith("__react"))return!1;let s=e[i];return s&&typeof s=="object"&&"_debugSource"in s});return r&&e[r]||null}function Ul(e){if(!e.type||typeof e.type=="string")return null;if(typeof e.type=="object"||typeof e.type=="function"){let t=e.type;if(t.displayName)return t.displayName;if(t.name)return t.name}return null}function pb(e,t=50){let n=e,o=0;for(;n&&o<t;){if(n._debugSource)return{source:n._debugSource,componentName:Ul(n)};if(n._debugOwner?._debugSource)return{source:n._debugOwner._debugSource,componentName:Ul(n._debugOwner)};n=n.return,o++}return null}function fb(e){let t=e,n=0,o=50;for(;t&&n<o;){let r=t,i=["_debugSource","__source","_source","debugSource"];for(let s of i){let l=r[s];if(l&&typeof l=="object"&&"fileName"in l)return{source:l,componentName:Ul(t)}}if(t.memoizedProps){let s=t.memoizedProps;if(s.__source&&typeof s.__source=="object"){let l=s.__source;if(l.fileName&&l.lineNumber)return{source:{fileName:l.fileName,lineNumber:l.lineNumber,columnNumber:l.columnNumber},componentName:Ul(t)}}}t=t.return,n++}return null}var uu=new Map;function gb(e){let t=e.tag,n=e.type,o=e.elementType;if(typeof n=="string"||n==null||typeof n=="function"&&n.prototype?.isReactComponent)return null;if((t===Wl.FunctionComponent||t===Wl.IndeterminateComponent)&&typeof n=="function")return n;if(t===Wl.ForwardRef&&o){let r=o.render;if(typeof r=="function")return r}if((t===Wl.MemoComponent||t===Wl.SimpleMemoComponent)&&o){let r=o.type;if(typeof r=="function")return r}return typeof n=="function"?n:null}function hb(){let e=_b,t=e.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE;if(t&&"H"in t)return{get:()=>t.H,set:o=>{t.H=o}};let n=e.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED;if(n){let o=n.ReactCurrentDispatcher;if(o&&"current"in o)return{get:()=>o.current,set:r=>{o.current=r}}}return null}function yb(e,t){let n=e.split(`
`),o=[/source-location/,/\/dist\/index\./,/node_modules\//,/react-dom/,/react\.development/,/react\.production/,/chunk-[A-Z0-9]+/i,/\/_next\/static\/chunks\//,/\/\.vite\/deps\//,/\/_astro\//,/\/assets\/[^\s/]+[-.][\w-]{8,}\.m?js(?:[?:]|$)/,/react-stack-bottom-frame/,/react-reconciler/,/scheduler/,/<anonymous>/],r=/^\s*at\s+(?:.*?\s+\()?(.+?):(\d+):(\d+)\)?$/,i=/^[^@]*@(.+?):(\d+):(\d+)$/;for(let s of n){let l=s.trim();if(!l||o.some(c=>c.test(l)))continue;if(t){let c=t.replace(/^bound /,"").replace(/[.*+?^${}()|[\]\\]/g,"\\$&");if(!new RegExp(`(?:at (?:Object\\.)?|^)${c}(?: \\(|@| \\[)`).test(l))continue}let a=r.exec(l)||i.exec(l);if(a)return{fileName:a[1],line:parseInt(a[2],10),column:parseInt(a[3],10)}}return null}function xb(e){let t=e;return t=t.replace(/[?#].*$/,""),t=t.replace(/^turbopack:\/\/\/\[project\]\//,""),t=t.replace(/^webpack-internal:\/\/\/\.\//,""),t=t.replace(/^webpack-internal:\/\/\//,""),t=t.replace(/^webpack:\/\/\/\.\//,""),t=t.replace(/^webpack:\/\/\//,""),t=t.replace(/^turbopack:\/\/\//,""),t=t.replace(/^https?:\/\/[^/]+\//,""),t=t.replace(/^file:\/\/\//,"/"),t=t.replace(/^\([^)]+\)\/\.\//,""),t=t.replace(/^\.\//,""),t}function vb(e){let t=gb(e);if(!t)return null;if(uu.has(t))return uu.get(t);let n=hb();if(!n)return uu.set(t,null),null;let o=n.get(),r=null;try{let i=new Proxy({},{get(){throw new Error("probe")}});n.set(i);try{t({})}catch(s){if(s instanceof Error&&s.message==="probe"&&s.stack){let l=yb(s.stack,t.name);l&&(r={fileName:xb(l.fileName),lineNumber:l.line,columnNumber:l.column,componentName:Ul(e)||void 0})}}}finally{n.set(o)}return uu.set(t,r),r}function wb(e,t=15){let n=e,o=0;for(;n&&o<t;){let r=vb(n);if(r)return r;n=n.return,o++}return null}function du(e){let t=mb(e);if(!t)return{found:!1,reason:"no-fiber",isReactApp:!1,isProduction:!1};let n=pb(t);if(n||(n=fb(t)),n?.source)return{found:!0,source:{fileName:n.source.fileName,lineNumber:n.source.lineNumber,columnNumber:n.source.columnNumber,componentName:n.componentName||void 0},isReactApp:!0,isProduction:!1};let o=wb(t);return o?{found:!0,source:o,isReactApp:!0,isProduction:!1}:{found:!1,reason:"no-debug-source",isReactApp:!0,isProduction:!1}}function p1(e,t="path"){let{fileName:n,lineNumber:o,columnNumber:r}=e,i=`${n}:${o}`;return r!==void 0&&(i+=`:${r}`),t==="vscode"?`vscode://file${n.startsWith("/")?"":"/"}${i}`:i}function f1(e,t=10){let n=e,o=0;for(;n&&o<t;){let r=du(n);if(r.found)return r;n=n.parentElement,o++}return du(e)}var ms=[{value:"compact",label:"Compact"},{value:"standard",label:"Standard"},{value:"detailed",label:"Detailed"},{value:"forensic",label:"Forensic"}];function jl(e,t){let n=`## Page Feedback: ${e}
`,o=t?.replace(/[\r\n\t]+/g," ").trim();return o&&(n+=`**App:** ${o.replace(/[\\`*_\[\]<>]/g,"\\$&")}
`),n}function Xm(e,t,n="standard",o={}){if(e.length===0)return"";let r=typeof window<"u"?`${window.innerWidth}\xD7${window.innerHeight}`:"unknown",i=jl(t,o.appName);n==="forensic"?(i+=`
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

`)}),i.trim()}var b1="[modern-screenshot]",Er=typeof window<"u",bb=Er&&"Worker"in window,MS=Er&&"atob"in window,TS=Er&&"btoa"in window,qm=Er?window.navigator?.userAgent:"",k1=qm.includes("Chrome"),_u=qm.includes("AppleWebKit")&&!k1,Km=qm.includes("Firefox"),kb=e=>e&&"__CONTEXT__"in e,Sb=e=>e.constructor.name==="CSSFontFaceRule",Cb=e=>e.constructor.name==="CSSImportRule",Eb=e=>e.constructor.name==="CSSLayerBlockRule",Ro=e=>e.nodeType===1,Ql=e=>typeof e.className=="object",S1=e=>e.tagName==="image",Mb=e=>e.tagName==="use",Yl=e=>Ro(e)&&typeof e.style<"u"&&!Ql(e),Tb=e=>e.nodeType===8,Lb=e=>e.nodeType===3,fs=e=>e.tagName==="IMG",mu=e=>e.tagName==="VIDEO",Rb=e=>e.tagName==="CANVAS",Pb=e=>e.tagName==="TEXTAREA",Nb=e=>e.tagName==="INPUT",Ib=e=>e.tagName==="STYLE",Db=e=>e.tagName==="SCRIPT",Ab=e=>e.tagName==="SELECT",$b=e=>e.tagName==="SLOT",Fb=e=>e.tagName==="IFRAME",Ob=(...e)=>console.warn(b1,...e);function Bb(e){let t=e?.createElement?.("canvas");return t&&(t.height=t.width=1),!!t&&"toDataURL"in t&&!!t.toDataURL("image/webp").includes("image/webp")}var Qm=e=>e.startsWith("data:");function C1(e,t){if(e.match(/^[a-z]+:\/\//i))return e;if(Er&&e.match(/^\/\//))return window.location.protocol+e;if(e.match(/^[a-z]+:/i)||!Er)return e;let n=pu().implementation.createHTMLDocument(),o=n.createElement("base"),r=n.createElement("a");return n.head.appendChild(o),n.body.appendChild(r),t&&(o.href=t),r.href=e,r.href}function pu(e){return(e&&Ro(e)?e?.ownerDocument:e)??window.document}var fu="http://www.w3.org/2000/svg";function zb(e,t,n){let o=pu(n).createElementNS(fu,"svg");return o.setAttributeNS(null,"width",e.toString()),o.setAttributeNS(null,"height",t.toString()),o.setAttributeNS(null,"viewBox",`0 0 ${e} ${t}`),o}function Hb(e,t){let n=new XMLSerializer().serializeToString(e);return t&&(n=n.replace(/[\u0000-\u0008\v\f\u000E-\u001F\uD800-\uDFFF\uFFFE\uFFFF]/gu,"")),`data:image/svg+xml;charset=utf-8,${encodeURIComponent(n)}`}function Wb(e,t){return new Promise((n,o)=>{let r=new FileReader;r.onload=()=>n(r.result),r.onerror=()=>o(r.error),r.onabort=()=>o(new Error(`Failed read blob to ${t}`)),t==="dataUrl"?r.readAsDataURL(e):t==="arrayBuffer"&&r.readAsArrayBuffer(e)})}var Ub=e=>Wb(e,"dataUrl");function ps(e,t){let n=pu(t).createElement("img");return n.decoding="sync",n.loading="eager",n.src=e,n}function Vl(e,t){return new Promise(n=>{let{timeout:o,ownerDocument:r,onError:i,onWarn:s}=t??{},l=typeof e=="string"?ps(e,pu(r)):e,a=null,c=null;function d(){n(l),a&&clearTimeout(a),c?.()}if(o&&(a=setTimeout(d,o)),mu(l)){let m=l.currentSrc||l.src;if(!m)return l.poster?Vl(l.poster,t).then(n):d();if(l.readyState>=2)return d();let f=d,k=w=>{s?.("Failed video load",m,w),i?.(w),d()};c=()=>{l.removeEventListener("loadeddata",f),l.removeEventListener("error",k)},l.addEventListener("loadeddata",f,{once:!0}),l.addEventListener("error",k,{once:!0})}else{let m=S1(l)?l.href.baseVal:l.currentSrc||l.src;if(!m)return d();let f=async()=>{if(fs(l)&&"decode"in l)try{await l.decode()}catch(w){s?.("Failed to decode image, trying to render anyway",l.dataset.originalSrc||m,w)}d()},k=w=>{s?.("Failed image load",l.dataset.originalSrc||m,w),d()};if(fs(l)&&l.complete)return f();c=()=>{l.removeEventListener("load",f),l.removeEventListener("error",k)},l.addEventListener("load",f,{once:!0}),l.addEventListener("error",k,{once:!0})}})}async function jb(e,t){Yl(e)&&(fs(e)||mu(e)?await Vl(e,t):await Promise.all(["img","video"].flatMap(n=>Array.from(e.querySelectorAll(n)).map(o=>Vl(o,t)))))}var E1=(function(){let t=0,n=()=>`0000${(Math.random()*36**4<<0).toString(36)}`.slice(-4);return()=>(t+=1,`u${n()}${t}`)})();function M1(e){return e?.split(",").map(t=>t.trim().replace(/"|'/g,"").toLowerCase()).filter(Boolean)}var g1=0;function Yb(e){let t=`${b1}[#${g1}]`;return g1++,{time:n=>e&&console.time(`${t} ${n}`),timeEnd:n=>e&&console.timeEnd(`${t} ${n}`),warn:(...n)=>e&&Ob(...n)}}function Vb(e){return{cache:e?"no-cache":"force-cache"}}async function T1(e,t){return kb(e)?e:Xb(e,{...t,autoDestruct:!0})}async function Xb(e,t){let{scale:n=1,workerUrl:o,workerNumber:r=1}=t||{},i=!!t?.debug,s=t?.features??!0,l=e.ownerDocument??(Er?window.document:void 0),a=e.ownerDocument?.defaultView??(Er?window:void 0),c=new Map,d={width:0,height:0,quality:1,type:"image/png",scale:n,backgroundColor:null,style:null,filter:null,maximumCanvasSize:0,timeout:3e4,progress:null,debug:i,fetch:{requestInit:Vb(t?.fetch?.bypassingCache),placeholderImage:"data:image/png;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7",bypassingCache:!1,...t?.fetch},fetchFn:null,font:{},drawImageInterval:100,workerUrl:null,workerNumber:r,onCloneEachNode:null,onCloneNode:null,onEmbedNode:null,onCreateForeignObjectSvg:null,includeStyleProperties:null,autoDestruct:!1,...t,__CONTEXT__:!0,log:Yb(i),node:e,ownerDocument:l,ownerWindow:a,dpi:n===1?null:96*n,svgStyleElement:L1(l),svgDefsElement:l?.createElementNS(fu,"defs"),svgStyles:new Map,defaultComputedStyles:new Map,workers:[...Array.from({length:bb&&o&&r?r:0})].map(()=>{try{let k=new Worker(o);return k.onmessage=async w=>{let{url:C,result:S}=w.data;S?c.get(C)?.resolve?.(S):c.get(C)?.reject?.(new Error(`Error receiving message from worker: ${C}`))},k.onmessageerror=w=>{let{url:C}=w.data;c.get(C)?.reject?.(new Error(`Error receiving message from worker: ${C}`))},k}catch(k){return d.log.warn("Failed to new Worker",k),null}}).filter(Boolean),fontFamilies:new Map,fontCssTexts:new Map,acceptOfImage:`${[Bb(l)&&"image/webp","image/svg+xml","image/*","*/*"].filter(Boolean).join(",")};q=0.8`,requests:c,drawImageCount:0,tasks:[],features:s,isEnable:k=>k==="restoreScrollPosition"?typeof s=="boolean"?!1:s[k]??!1:typeof s=="boolean"?s:s[k]??!0,shadowRoots:[]};d.log.time("wait until load"),await jb(e,{timeout:d.timeout,onWarn:d.log.warn}),d.log.timeEnd("wait until load");let{width:m,height:f}=Qb(e,d);return d.width=m,d.height=f,d}function L1(e){if(!e)return;let t=e.createElement("style"),n=t.ownerDocument.createTextNode(`
.______background-clip--text {
  background-clip: text;
  -webkit-background-clip: text;
}
`);return t.appendChild(n),t}function Qb(e,t){let{width:n,height:o}=t;if(Ro(e)&&(!n||!o)){let r=e.getBoundingClientRect();n=n||r.width||Number(e.getAttribute("width"))||0,o=o||r.height||Number(e.getAttribute("height"))||0}return{width:n,height:o}}async function Gb(e,t){let{log:n,timeout:o,drawImageCount:r,drawImageInterval:i}=t;n.time("image to canvas");let s=await Vl(e,{timeout:o,onWarn:t.log.warn}),{canvas:l,context2d:a}=qb(e.ownerDocument,t),c=()=>{try{a?.drawImage(s,0,0,l.width,l.height)}catch(d){t.log.warn("Failed to drawImage",d)}};if(c(),t.isEnable("fixSvgXmlDecode"))for(let d=0;d<r;d++)await new Promise(m=>{setTimeout(()=>{a?.clearRect(0,0,l.width,l.height),c(),m()},d+i)});return t.drawImageCount=0,n.timeEnd("image to canvas"),l}function qb(e,t){let{width:n,height:o,scale:r,backgroundColor:i,maximumCanvasSize:s}=t,l=e.createElement("canvas");l.width=Math.floor(n*r),l.height=Math.floor(o*r),l.style.width=`${n}px`,l.style.height=`${o}px`,s&&(l.width>s||l.height>s)&&(l.width>s&&l.height>s?l.width>l.height?(l.height*=s/l.width,l.width=s):(l.width*=s/l.height,l.height=s):l.width>s?(l.height*=s/l.width,l.width=s):(l.width*=s/l.height,l.height=s));let a=l.getContext("2d");return a&&i&&(a.fillStyle=i,a.fillRect(0,0,l.width,l.height)),{canvas:l,context2d:a}}function R1(e,t){if(e.ownerDocument)try{let i=e.toDataURL();if(i!=="data:,")return ps(i,e.ownerDocument)}catch(i){t.log.warn("Failed to clone canvas",i)}let n=e.cloneNode(!1),o=e.getContext("2d"),r=n.getContext("2d");try{return o&&r&&r.putImageData(o.getImageData(0,0,e.width,e.height),0,0),n}catch(i){t.log.warn("Failed to clone canvas",i)}return n}function Kb(e,t){try{if(e?.contentDocument?.documentElement)return Jm(e.contentDocument.documentElement,t)}catch(n){t.log.warn("Failed to clone iframe",n)}return e.cloneNode(!1)}function Jb(e){let t=e.cloneNode(!1);return e.currentSrc&&e.currentSrc!==e.src&&(t.src=e.currentSrc,t.srcset=""),t.loading==="lazy"&&(t.loading="eager"),t}async function Zb(e,t){if(e.ownerDocument&&!e.currentSrc&&e.poster)return ps(e.poster,e.ownerDocument);let n=e.cloneNode(!1);n.crossOrigin="anonymous",e.currentSrc&&e.currentSrc!==e.src&&(n.src=e.currentSrc);let o=n.ownerDocument;if(o){let r=!0;if(await Vl(n,{onError:()=>r=!1,onWarn:t.log.warn}),!r)return e.poster?ps(e.poster,e.ownerDocument):n;n.currentTime=e.currentTime,await new Promise(s=>{n.addEventListener("seeked",s,{once:!0})});let i=o.createElement("canvas");i.width=e.offsetWidth,i.height=e.offsetHeight;try{let s=i.getContext("2d");s&&s.drawImage(n,0,0,i.width,i.height)}catch(s){return t.log.warn("Failed to clone video",s),e.poster?ps(e.poster,e.ownerDocument):n}return R1(i,t)}return n}function ek(e,t){return Rb(e)?R1(e,t):Fb(e)?Kb(e,t):fs(e)?Jb(e):mu(e)?Zb(e,t):e.cloneNode(!1)}function tk(e){let t=e.sandbox;if(!t){let{ownerDocument:n}=e;try{n&&(t=n.createElement("iframe"),t.id=`__SANDBOX__${E1()}`,t.width="0",t.height="0",t.style.visibility="hidden",t.style.position="fixed",n.body.appendChild(t),t.srcdoc='<!DOCTYPE html><meta charset="UTF-8"><title></title><body>',e.sandbox=t)}catch(o){e.log.warn("Failed to getSandBox",o)}}return t}var nk=["width","height","-webkit-text-fill-color"],ok=["stroke","fill"];function P1(e,t,n){let{defaultComputedStyles:o}=n,r=e.nodeName.toLowerCase(),i=Ql(e)&&r!=="svg",s=i?ok.map(C=>[C,e.getAttribute(C)]).filter(([,C])=>C!==null):[],l=[i&&"svg",r,s.map((C,S)=>`${C}=${S}`).join(","),t].filter(Boolean).join(":");if(o.has(l))return o.get(l);let c=tk(n)?.contentWindow;if(!c)return new Map;let d=c?.document,m,f;i?(m=d.createElementNS(fu,"svg"),f=m.ownerDocument.createElementNS(m.namespaceURI,r),s.forEach(([C,S])=>{f.setAttributeNS(null,C,S)}),m.appendChild(f)):m=f=d.createElement(r),f.textContent=" ",d.body.appendChild(m);let k=c.getComputedStyle(f,t),w=new Map;for(let C=k.length,S=0;S<C;S++){let g=k.item(S);nk.includes(g)||w.set(g,k.getPropertyValue(g))}return d.body.removeChild(m),o.set(l,w),w}function N1(e,t,n){let o=new Map,r=[],i=new Map;if(n)for(let l of n)s(l);else for(let l=e.length,a=0;a<l;a++){let c=e.item(a);s(c)}for(let l=r.length,a=0;a<l;a++)i.get(r[a])?.forEach((c,d)=>o.set(d,c));function s(l){let a=e.getPropertyValue(l),c=e.getPropertyPriority(l),d=l.lastIndexOf("-"),m=d>-1?l.substring(0,d):void 0;if(m){let f=i.get(m);f||(f=new Map,i.set(m,f)),f.set(l,[a,c])}t.get(l)===a&&!c||(m?r.push(m):o.set(l,[a,c]))}return o}function rk(e,t,n,o){let{ownerWindow:r,includeStyleProperties:i,currentParentNodeStyle:s}=o,l=t.style,a=r.getComputedStyle(e),c=P1(e,null,o);s?.forEach((m,f)=>{c.delete(f)});let d=N1(a,c,i);d.delete("transition-property"),d.delete("all"),d.delete("d"),d.delete("content"),n&&(d.delete("position"),d.delete("margin-top"),d.delete("margin-right"),d.delete("margin-bottom"),d.delete("margin-left"),d.delete("margin-block-start"),d.delete("margin-block-end"),d.delete("margin-inline-start"),d.delete("margin-inline-end"),d.set("box-sizing",["border-box",""])),d.get("background-clip")?.[0]==="text"&&t.classList.add("______background-clip--text"),k1&&(d.has("font-kerning")||d.set("font-kerning",["normal",""]),(d.get("overflow-x")?.[0]==="hidden"||d.get("overflow-y")?.[0]==="hidden")&&d.get("text-overflow")?.[0]==="ellipsis"&&e.scrollWidth===e.clientWidth&&d.set("text-overflow",["clip",""]));for(let m=l.length,f=0;f<m;f++)l.removeProperty(l.item(f));return d.forEach(([m,f],k)=>{l.setProperty(k,m,f)}),d}function ik(e,t){(Pb(e)||Nb(e)||Ab(e))&&t.setAttribute("value",e.value)}var sk=["::before","::after"],lk=["::-webkit-scrollbar","::-webkit-scrollbar-button","::-webkit-scrollbar-thumb","::-webkit-scrollbar-track","::-webkit-scrollbar-track-piece","::-webkit-scrollbar-corner","::-webkit-resizer"];function ak(e,t,n,o,r){let{ownerWindow:i,svgStyleElement:s,svgStyles:l,currentNodeStyle:a}=o;if(!s||!i)return;function c(d){let m=i.getComputedStyle(e,d),f=m.getPropertyValue("content");if(!f||f==="none")return;r?.(f),f=f.replace(/(')|(")|(counter\(.+\))/g,"");let k=[E1()],w=P1(e,d,o);a?.forEach((h,E)=>{w.delete(E)});let C=N1(m,w,o.includeStyleProperties);C.delete("content"),C.delete("-webkit-locale"),C.get("background-clip")?.[0]==="text"&&t.classList.add("______background-clip--text");let S=[`content: '${f}';`];if(C.forEach(([h,E],O)=>{S.push(`${O}: ${h}${E?" !important":""};`)}),S.length===1)return;try{t.className=[t.className,...k].join(" ")}catch(h){o.log.warn("Failed to copyPseudoClass",h);return}let g=S.join(`
  `),x=l.get(g);x||(x=[],l.set(g,x)),x.push(`.${k[0]}${d}`)}sk.forEach(c),n&&lk.forEach(c)}var h1=new Set(["symbol"]);async function y1(e,t,n,o,r){if(Ro(n)&&(Ib(n)||Db(n))||o.filter&&!o.filter(n))return;h1.has(t.nodeName)||h1.has(n.nodeName)?o.currentParentNodeStyle=void 0:o.currentParentNodeStyle=o.currentNodeStyle;let i=await Jm(n,o,!1,r);o.isEnable("restoreScrollPosition")&&ck(e,i),t.appendChild(i)}async function x1(e,t,n,o){let r=e.firstChild;Ro(e)&&e.shadowRoot&&(r=e.shadowRoot?.firstChild,n.shadowRoots.push(e.shadowRoot));for(let i=r;i;i=i.nextSibling)if(!Tb(i))if(Ro(i)&&$b(i)&&typeof i.assignedNodes=="function"){let s=i.assignedNodes();for(let l=0;l<s.length;l++)await y1(e,t,s[l],n,o)}else await y1(e,t,i,n,o)}function ck(e,t){if(!Yl(e)||!Yl(t))return;let{scrollTop:n,scrollLeft:o}=e;if(!n&&!o)return;let{transform:r}=t.style,i=new DOMMatrix(r),{a:s,b:l,c:a,d:c}=i;i.a=1,i.b=0,i.c=0,i.d=1,i.translateSelf(-o,-n),i.a=s,i.b=l,i.c=a,i.d=c,t.style.transform=i.toString()}function uk(e,t){let{backgroundColor:n,width:o,height:r,style:i}=t,s=e.style;if(n&&s.setProperty("background-color",n,"important"),o&&s.setProperty("width",`${o}px`,"important"),r&&s.setProperty("height",`${r}px`,"important"),i)for(let l in i)s[l]=i[l]}var dk=/^[\w-:]+$/;async function Jm(e,t,n=!1,o){let{ownerDocument:r,ownerWindow:i,fontFamilies:s,onCloneEachNode:l}=t;if(r&&Lb(e))return o&&/\S/.test(e.data)&&o(e.data),r.createTextNode(e.data);if(r&&i&&Ro(e)&&(Yl(e)||Ql(e))){let c=await ek(e,t);if(t.isEnable("removeAbnormalAttributes")){let C=c.getAttributeNames();for(let S=C.length,g=0;g<S;g++){let x=C[g];dk.test(x)||c.removeAttribute(x)}}let d=t.currentNodeStyle=rk(e,c,n,t);n&&uk(c,t);let m=!1;if(t.isEnable("copyScrollbar")){let C=[d.get("overflow-x")?.[0],d.get("overflow-y")?.[0]];m=C.includes("scroll")||(C.includes("auto")||C.includes("overlay"))&&(e.scrollHeight>e.clientHeight||e.scrollWidth>e.clientWidth)}let f=d.get("text-transform")?.[0],k=M1(d.get("font-family")?.[0]),w=k?C=>{f==="uppercase"?C=C.toUpperCase():f==="lowercase"?C=C.toLowerCase():f==="capitalize"&&(C=C[0].toUpperCase()+C.substring(1)),k.forEach(S=>{let g=s.get(S);g||s.set(S,g=new Set),C.split("").forEach(x=>g.add(x))})}:void 0;return ak(e,c,m,t,w),ik(e,c),mu(e)||await x1(e,c,t,w),await l?.(c),c}let a=e.cloneNode(!1);return await x1(e,a,t),await l?.(a),a}function _k(e){if(e.ownerDocument=void 0,e.ownerWindow=void 0,e.svgStyleElement=void 0,e.svgDefsElement=void 0,e.svgStyles.clear(),e.defaultComputedStyles.clear(),e.sandbox){try{e.sandbox.remove()}catch(t){e.log.warn("Failed to destroyContext",t)}e.sandbox=void 0}e.workers=[],e.fontFamilies.clear(),e.fontCssTexts.clear(),e.requests.clear(),e.tasks=[],e.shadowRoots=[]}function mk(e){let{url:t,timeout:n,responseType:o,...r}=e,i=new AbortController,s=n?setTimeout(()=>i.abort(),n):void 0;return fetch(t,{signal:i.signal,...r}).then(l=>{if(!l.ok)throw new Error("Failed fetch, not 2xx response",{cause:l});switch(o){case"arrayBuffer":return l.arrayBuffer();case"dataUrl":return l.blob().then(Ub);default:return l.text()}}).finally(()=>clearTimeout(s))}function Xl(e,t){let{url:n,requestType:o="text",responseType:r="text",imageDom:i}=t,s=n,{timeout:l,acceptOfImage:a,requests:c,fetchFn:d,fetch:{requestInit:m,bypassingCache:f,placeholderImage:k},font:w,workers:C,fontFamilies:S}=e;o==="image"&&(_u||Km)&&e.drawImageCount++;let g=c.get(n);if(!g){f&&f instanceof RegExp&&f.test(s)&&(s+=(/\?/.test(s)?"&":"?")+new Date().getTime());let x=o.startsWith("font")&&w&&w.minify,h=new Set;x&&o.split(";")[1].split(",").forEach(A=>{S.has(A)&&S.get(A).forEach(Y=>h.add(Y))});let E=x&&h.size,O={url:s,timeout:l,responseType:E?"arrayBuffer":r,headers:o==="image"?{accept:a}:void 0,...m};g={type:o,resolve:void 0,reject:void 0,response:null},g.response=(async()=>{if(d&&o==="image"){let j=await d(n);if(j)return j}return!_u&&n.startsWith("http")&&C.length?new Promise((j,A)=>{C[c.size&C.length-1].postMessage({rawUrl:n,...O}),g.resolve=j,g.reject=A}):mk(O)})().catch(j=>{if(c.delete(n),o==="image"&&k)return e.log.warn("Failed to fetch image base64, trying to use placeholder image",s),typeof k=="string"?k:k(i);throw j}),c.set(n,g)}return g.response}async function I1(e,t,n,o){if(!D1(e))return e;for(let[r,i]of pk(e,t))try{let s=await Xl(n,{url:i,requestType:o?"image":"text",responseType:"dataUrl"});e=e.replace(fk(r),`$1${s}$3`)}catch(s){n.log.warn("Failed to fetch css data url",r,s)}return e}function D1(e){return/url\((['"]?)([^'"]+?)\1\)/.test(e)}var A1=/url\((['"]?)([^'"]+?)\1\)/g;function pk(e,t){let n=[];return e.replace(A1,(o,r,i)=>(n.push([i,C1(i,t)]),o)),n.filter(([o])=>!Qm(o))}function fk(e){let t=e.replace(/([.*+?^${}()|\[\]\/\\])/g,"\\$1");return new RegExp(`(url\\(['"]?)(${t})(['"]?\\))`,"g")}var gk=["background-image","border-image-source","-webkit-border-image","-webkit-mask-image","list-style-image"];function hk(e,t){return gk.map(n=>{let o=e.getPropertyValue(n);return!o||o==="none"?null:((_u||Km)&&t.drawImageCount++,I1(o,null,t,!0).then(r=>{!r||o===r||e.setProperty(n,r,e.getPropertyPriority(n))}))}).filter(Boolean)}function yk(e,t){if(fs(e)){let n=e.currentSrc||e.src;if(!Qm(n))return[Xl(t,{url:n,imageDom:e,requestType:"image",responseType:"dataUrl"}).then(o=>{o&&(e.srcset="",e.dataset.originalSrc=n,e.src=o||"")})];(_u||Km)&&t.drawImageCount++}else if(Ql(e)&&!Qm(e.href.baseVal)){let n=e.href.baseVal;return[Xl(t,{url:n,imageDom:e,requestType:"image",responseType:"dataUrl"}).then(o=>{o&&(e.dataset.originalSrc=n,e.href.baseVal=o||"")})]}return[]}function xk(e,t){let{ownerDocument:n,svgDefsElement:o}=t,r=e.getAttribute("href")??e.getAttribute("xlink:href");if(!r)return[];let[i,s]=r.split("#");if(s){let l=`#${s}`,a=t.shadowRoots.reduce((c,d)=>c??d.querySelector(`svg ${l}`),n?.querySelector(`svg ${l}`));if(i&&e.setAttribute("href",l),o?.querySelector(l))return[];if(a)return o?.appendChild(a.cloneNode(!0)),[];if(i)return[Xl(t,{url:i,responseType:"text"}).then(c=>{o?.insertAdjacentHTML("beforeend",c)})]}return[]}function $1(e,t){let{tasks:n}=t;Ro(e)&&((fs(e)||S1(e))&&n.push(...yk(e,t)),Mb(e)&&n.push(...xk(e,t))),Yl(e)&&n.push(...hk(e.style,t)),e.childNodes.forEach(o=>{$1(o,t)})}async function vk(e,t){let{ownerDocument:n,svgStyleElement:o,fontFamilies:r,fontCssTexts:i,tasks:s,font:l}=t;if(!(!n||!o||!r.size))if(l&&l.cssText){let a=w1(l.cssText,t);o.appendChild(n.createTextNode(`${a}
`))}else{let a=Array.from(n.styleSheets).filter(k=>{try{return"cssRules"in k&&!!k.cssRules.length}catch(w){return t.log.warn(`Error while reading CSS rules from ${k.href}`,w),!1}}),c=n.implementation.createHTMLDocument(""),d=c.createElement("style");c.head.appendChild(d);let m=d.sheet;await Promise.all(a.flatMap(k=>Array.from(k.cssRules).map(async w=>{if(Cb(w)){let C=w.href,S="";try{S=await Xl(t,{url:C,requestType:"text",responseType:"text"})}catch(x){t.log.warn(`Error fetch remote css import from ${C}`,x)}let g=S.replace(A1,(x,h,E)=>x.replace(E,C1(E,C)));for(let x of bk(g))try{m.insertRule(x,m.cssRules.length)}catch(h){t.log.warn("Error inserting rule from remote css import",{rule:x,error:h})}}}))),m.cssRules.length&&a.push(m);let f=[];a.forEach(k=>{Gm(k.cssRules,f)}),f.filter(k=>Sb(k)&&D1(k.style.getPropertyValue("src"))&&M1(k.style.getPropertyValue("font-family"))?.some(w=>r.has(w))).forEach(k=>{let w=k,C=i.get(w.cssText);C?o.appendChild(n.createTextNode(`${C}
`)):s.push(I1(w.cssText,w.parentStyleSheet?w.parentStyleSheet.href:null,t).then(S=>{S=w1(S,t),i.set(w.cssText,S),o.appendChild(n.createTextNode(`${S}
`))}))})}}var wk=/(\/\*[\s\S]*?\*\/)/g,v1=/((@.*?keyframes [\s\S]*?){([\s\S]*?}\s*?)})/gi;function bk(e){if(e==null)return[];let t=[],n=e.replace(wk,"");for(;;){let i=v1.exec(n);if(!i)break;t.push(i[0])}n=n.replace(v1,"");let o=/@import[\s\S]*?url\([^)]*\)[\s\S]*?;/gi,r=new RegExp("((\\s*?(?:\\/\\*[\\s\\S]*?\\*\\/)?\\s*?@media[\\s\\S]*?){([\\s\\S]*?)}\\s*?})|(([\\s\\S]*?){([\\s\\S]*?)})","gi");for(;;){let i=o.exec(n);if(i)r.lastIndex=o.lastIndex;else if(i=r.exec(n),i)o.lastIndex=r.lastIndex;else break;t.push(i[0])}return t}var kk=/url\([^)]+\)\s*format\((["']?)([^"']+)\1\)/g,Sk=/src:\s*(?:url\([^)]+\)\s*format\([^)]+\)[,;]\s*)+/g;function w1(e,t){let{font:n}=t,o=n?n?.preferredFormat:void 0;return o?e.replace(Sk,r=>{for(;;){let[i,,s]=kk.exec(r)||[];if(!s)return"";if(s===o)return`src: ${i};`}}):e}function Gm(e,t=[]){for(let n of Array.from(e))Eb(n)?t.push(...Gm(n.cssRules)):"cssRules"in n?Gm(n.cssRules,t):t.push(n);return t}var Ck=/\bx?link:?href\s*=\s*["'](?!data:)[^"']+["']/i;function Ek(e){return Ck.test(e.innerHTML)}async function Mk(e,t){let n=await T1(e,t);if(Ro(n.node)&&Ql(n.node)&&!Ek(n.node))return n.node;let{ownerDocument:o,log:r,tasks:i,svgStyleElement:s,svgDefsElement:l,svgStyles:a,font:c,progress:d,autoDestruct:m,onCloneNode:f,onEmbedNode:k,onCreateForeignObjectSvg:w}=n;r.time("clone node");let C=await Jm(n.node,n,!0);if(s&&o){let E="";a.forEach((O,j)=>{E+=`${O.join(`,
`)} {
  ${j}
}
`}),s.appendChild(o.createTextNode(E))}r.timeEnd("clone node"),await f?.(C),c!==!1&&Ro(C)&&(r.time("embed web font"),await vk(C,n),r.timeEnd("embed web font")),r.time("embed node"),$1(C,n);let S=i.length,g=0,x=async()=>{for(;;){let E=i.pop();if(!E)break;try{await E}catch(O){n.log.warn("Failed to run task",O)}d?.(++g,S)}};d?.(g,S),await Promise.all([...Array.from({length:4})].map(x)),r.timeEnd("embed node"),await k?.(C);let h=Tk(C,n);return l&&h.insertBefore(l,h.children[0]),s&&h.insertBefore(s,h.children[0]),m&&_k(n),await w?.(h),h}function Tk(e,t){let{width:n,height:o}=t,r=zb(n,o,e.ownerDocument),i=r.ownerDocument.createElementNS(r.namespaceURI,"foreignObject");return i.setAttributeNS(null,"x","0%"),i.setAttributeNS(null,"y","0%"),i.setAttributeNS(null,"width","100%"),i.setAttributeNS(null,"height","100%"),i.append(e),r.appendChild(i),r}async function F1(e,t){let n=await T1(e,t),o=await Mk(n),r=Hb(o,n.isEnable("removeControlCharacter"));n.autoDestruct||(n.svgStyleElement=L1(n.ownerDocument),n.svgDefsElement=n.ownerDocument?.createElementNS(fu,"defs"),n.svgStyles.clear());let i=ps(r,o.ownerDocument);return await Gb(i,n)}var Lk=1600,O1=2,z1="[data-agentation-portal], [data-agentation-root], [data-agentation-toolbar], [data-annotation-popup], agentation-toolbar";function Rk(e){return e instanceof Element?e.matches(z1):!1}function Pk(e){let t=document,n=Math.min(Math.max(e.x+e.width/2,0),window.innerWidth-1),o=Math.min(Math.max(e.y+e.height/2,0),window.innerHeight-1),r=t.elementsFromPoint(n,o);for(let i of r){if(!(i instanceof HTMLElement))continue;if(i===t.documentElement||i===t.body)break;if(i.closest(z1))continue;let s=i;for(;s&&s!==t.body&&s!==t.documentElement;){if(s.scrollTop!==0||s.scrollLeft!==0){s=null;break}s=s.parentElement}if(!s)continue;let l=i.getBoundingClientRect();if(l.left<=e.x&&l.top<=e.y&&l.right>=e.x+e.width&&l.bottom>=e.y+e.height)return i}return t.documentElement}function B1(){for(let e of[document.body,document.documentElement]){if(!e)continue;let t=getComputedStyle(e).backgroundColor;if(t&&t!=="transparent"&&!/rgba\(.*,\s*0\)$/.test(t))return t}return"#ffffff"}function Nk(e,t,n,o,r,i){let s=Math.max(0,Math.min(i,o/2,r/2));e.beginPath(),e.moveTo(t+s,n),e.arcTo(t+o,n,t+o,n+r,s),e.arcTo(t+o,n+r,t,n+r,s),e.arcTo(t,n+r,t,n,s),e.arcTo(t,n,t+o,n,s),e.closePath(),e.stroke()}async function H1(e){if(typeof window>"u"||typeof document>"u")return null;let{rect:t}=e;if(t.width<O1&&t.height<O1)return null;let n=e.padding??24,o=e.minWidth??640,r=e.minHeight??400,i=-window.scrollX,s=-window.scrollY,l=i+Math.max(document.documentElement.scrollWidth,window.innerWidth),a=s+Math.max(document.documentElement.scrollHeight,window.innerHeight),c=(h,E,O,j,A)=>{let Y=h-n,K=h+E+n;if(K-Y<O){let q=(O-(K-Y))/2;Y-=q,K+=q}return Y<j&&(K+=j-Y,Y=j),K>A&&(Y-=K-A,K=A),Y=Math.max(Y,j),[Y,K-Y]},[d,m]=c(t.x,t.width,o,i,l),[f,k]=c(t.y,t.height,r,s,a),w={x:d,y:f,width:m,height:k};if(w.width<1||w.height<1)return null;let C=Pk(w),S=C.getBoundingClientRect(),g=Math.max(2,Math.min(window.devicePixelRatio||1,3)),x=Math.min(g,Lk/Math.max(w.width,w.height));try{let h=await F1(C,{scale:g,backgroundColor:B1(),filter:q=>!Rk(q),timeout:4e3});if(!h.width||!h.height||!S.width)return null;let E=h.width/Math.max(C.scrollWidth,S.width),O=Math.max(1,Math.round(w.width*x)),j=Math.max(1,Math.round(w.height*x)),A=document.createElement("canvas");A.width=O,A.height=j;let Y=A.getContext("2d");if(!Y)return null;Y.fillStyle=B1(),Y.fillRect(0,0,O,j),Y.drawImage(h,(w.x-S.left)*E,(w.y-S.top)*E,w.width*E,w.height*E,0,0,O,j);let K=e.highlights?.length?e.highlights:[t];Y.strokeStyle=e.accentColor??"#3c82f7",Y.lineWidth=Math.max(2,2*x);for(let q of K)Nk(Y,(q.x-3-w.x)*x,(q.y-3-w.y)*x,(q.width+6)*x,(q.height+6)*x,6*x);return A.toDataURL("image/png")}catch(h){return console.warn("[Agentation] Screenshot capture failed:",h),null}}function Zm(e,t,n="markdown"){return n==="markdown"?t:[...new Set(e.map(o=>n==="source"?o.sourceFile:n==="classes"?o.cssClasses:o.attributes?.[n.attribute]).filter(o=>typeof o=="string"&&o.length>0))].join(`
`)}async function W1(e){if(typeof window>"u")return!1;try{if(navigator.clipboard?.writeText)return await navigator.clipboard.writeText(e),!0}catch{}return Ik(e)}function Ik(e){let t=document.createElement("textarea"),n=document.activeElement;for(;n?.shadowRoot?.activeElement;)n=n.shadowRoot.activeElement;let o=n instanceof HTMLInputElement||n instanceof HTMLTextAreaElement?n:null,r=o&&o.selectionStart!==null?{start:o.selectionStart,end:o.selectionEnd,direction:o.selectionDirection}:null,i=document.getSelection(),s=i?Array.from({length:i.rangeCount},(l,a)=>i.getRangeAt(a).cloneRange()):[];try{return t.value=e,t.setAttribute("readonly",""),t.style.cssText="position:fixed;left:-9999px;top:0;opacity:0;pointer-events:none;",document.body.appendChild(t),t.focus({preventScroll:!0}),t.select(),t.setSelectionRange(0,e.length),document.execCommand("copy")}catch{return!1}finally{if(t.remove(),n instanceof HTMLElement&&n.isConnected&&(n.focus({preventScroll:!0}),o&&r&&o.setSelectionRange(r.start,r.end,r.direction)),i){i.removeAllRanges();for(let l of s)i.addRange(l)}}}function U1(e){if(!e)return e;try{let t=new URL(e,"http://agentation.invalid");return t.pathname+t.search+t.hash}catch{return e}}function ep(e,t,n=[]){let o=new Map,r=new Map(n.map(l=>[l.id,l])),i=!1;async function s(l,a){if(i||a.running||a.timer)return;let c=a.desired,d=t.get(l);if(!c&&!d){o.delete(l),t.delete(l);return}let m=c?JSON.stringify(c):void 0;if(!(c&&d&&a.synced===m)){a.running=!0;try{if(!c)await e.remove(d),t.delete(l),a.synced=void 0;else if(d)await e.update(d,c),a.synced=m;else{t.set(l,"");let f=await e.create(c);t.set(l,f.id),a.synced=m}a.retries=0,a.running=!1,!i&&o.get(l)===a&&s(l,a)}catch(f){if(a.running=!1,!i&&o.get(l)===a&&(console.warn("[Agentation] Failed to sync layout feedback:",f),t.get(l)&&a.retries<3)){let k=500*2**a.retries++;a.timer=Ne(()=>{a.timer=void 0,s(l,a)},k)}}}}return{replace(l){let a=new Map(l.map(c=>[c.id,c]));for(let[c,d]of a){let m=o.get(c);if(!m){m={running:!1,retries:0},o.set(c,m);let f=[...r.values()].find(k=>k.kind===d.kind&&U1(k.url)===U1(d.url)&&(d.kind==="placement"?k.timestamp===d.timestamp&&k.element===d.element:k.element===d.element));f&&(t.set(c,f.id),r.delete(f.id))}JSON.stringify(m.desired)!==JSON.stringify(d)&&(m.retries=0,m.timer&&clearTimeout(m.timer),m.timer=void 0),m.desired=d}for(let[c,d]of o)a.has(c)||(d.desired=void 0),s(c,d)},forget(l){let a=o.get(l);a?.timer&&clearTimeout(a.timer),o.delete(l),t.delete(l)},dispose(){i=!0;for(let l of o.values())l.timer&&clearTimeout(l.timer)}}}function j1(e,t,n,o){let r=!1,i,s,l=1e3,a,c=k=>{!r&&(k?.status==="resolved"||k?.status==="dismissed")&&o(k)},d=async()=>{if(r||a||!n())return;let k=new AbortController;a=k;let w=Ne(()=>k.abort(),5e3);try{let C=await fetch(`${e}/sessions/${t}`,{signal:k.signal});if(!C.ok)return;let S=await C.json();!r&&!k.signal.aborted&&Array.isArray(S.annotations)&&S.annotations.forEach(c)}catch{}finally{clearTimeout(w),a===k&&(a=void 0)}},m=()=>{if(r)return;let k=new EventSource(`${e}/sessions/${t}/events`),w=()=>{l=1e3,d()},C=g=>{try{c(JSON.parse(g.data).payload)}catch{}},S=()=>{k.readyState!==EventSource.CLOSED||r||s!==void 0||(i?.(),s=Ne(()=>{s=void 0,m()},l),l=Math.min(l*2,1e4))};k.addEventListener("open",w),k.addEventListener("annotation.updated",C),k.addEventListener("error",S),i=()=>{k.removeEventListener("open",w),k.removeEventListener("annotation.updated",C),k.removeEventListener("error",S),k.close()}};m();let f=Yc(()=>{d()},1e4);return()=>{r=!0,i?.(),s!==void 0&&clearTimeout(s),clearInterval(f),a?.abort()}}var Ln=ae(bt());var Y1=`.styles-module__surface___7qnpJ {
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
}`,V1={surface:"styles-module__surface___7qnpJ"};var tp=ae(Be()),X1=(0,Ln.forwardRef)(function({annotation:t,editing:n,exiting:o,restorePreview:r,editorProps:i,lightMode:s,scrollY:l,onExited:a},c){let d=(0,Ln.useRef)({annotation:t,editorProps:i}),m=t??d.current.annotation,f=t?i:d.current.editorProps,k=(0,Ln.useRef)(null),w=(0,Ln.useRef)(null),C=(0,Ln.useRef)(),S=(0,Ln.useRef)(l),g=n&&!o?"edit":t&&(!n||r)?"preview":"hidden",x=g==="preview"||!n;return(0,Ln.useLayoutEffect)(()=>{t&&(d.current={annotation:t,editorProps:i})},[t,i]),(0,Ln.useLayoutEffect)(()=>{let h=k.current;if(!h||!m)return;let E=g==="edit"&&(C.current!==m.id||h.dataset.state==="hidden"&&getComputedStyle(h).opacity==="0");E&&(h.dataset.directEntry="true");let O=()=>{let Y=m.x/100*window.innerWidth,K=m.isFixed?m.y:m.y-l,q=g==="preview"||!n,Z=parseFloat(h.style.getPropertyValue("--preview-width"))||200,Ie=Math.min(Z,window.innerWidth-24),ye=Math.max(12,Math.min(window.innerWidth-Ie-12,Y-Ie/2)),me=q?Ie:Math.min(280,window.innerWidth-24),Me=Math.min(q?12:20,(window.innerWidth-me)/2),B=K>window.innerHeight-(q?101:290);h.style.left=`${Math.max(Me,Math.min(window.innerWidth-me-Me,ye))}px`,h.style.right="auto",h.style.top=`${Math.max(12,Math.min(window.innerHeight-12,K+(B?-21:21)))}px`,h.style.bottom="auto",h.style.transform=B?"translateY(-100%)":"translateY(0)",h.style.maxHeight=`${Math.max(100,B?K-33:window.innerHeight-K-33)}px`},j=C.current!==m.id||S.current!==l||h.dataset.state==="hidden";j&&(h.dataset.positioning="true"),O(),(j||E)&&h.getBoundingClientRect(),delete h.dataset.positioning,delete h.dataset.directEntry,h.dataset.state=g,h.inert=g!=="edit",C.current=m.id,S.current=l;let A=()=>{h.dataset.positioning="true",O(),h.getBoundingClientRect(),delete h.dataset.positioning};return window.addEventListener("resize",A),()=>window.removeEventListener("resize",A)},[m?.id,m?.x,m?.y,m?.isFixed,g,n,l,m?.comment]),(0,Ln.useLayoutEffect)(()=>{n&&!o&&w.current?.focus()},[n,o,m?.id]),as(k,o,a),(0,Ln.useImperativeHandle)(c,()=>({shake(){k.current?.animate?.([{translate:"0px"},{translate:"-3px"},{translate:"3px"},{translate:"-2px"},{translate:"2px"},{translate:"0px"}],{duration:250}),w.current?.focus()}}),[]),!m||!f?null:(0,tp.jsx)("div",{ref:k,className:`${Re.popup} ${V1.surface} ${s?Re.light:""}`,"data-feedback-toolbar":!0,"data-annotation-card":!0,"data-annotation-popup":n?"":void 0,"data-state":"hidden","aria-hidden":g==="hidden",onClick:h=>h.stopPropagation(),onKeyDownCapture:h=>{h.key!=="Escape"||h.nativeEvent.isComposing||!n||(h.preventDefault(),h.stopPropagation(),f.onCancel())},children:(0,tp.jsx)(eu,{ref:w,...f,variant:"card",preview:x,resetOnPreview:!n,disabled:!n||o},m.id)})});var Jn=ae(bt());var Q1=`@keyframes styles-module__markerIn___x4G8D {
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
}`,Jt={marker:"styles-module__marker___9CKF7",previewVisible:"styles-module__previewVisible___imMag",enter:"styles-module__enter___8kI3q",exit:"styles-module__exit___KBdR3",clearing:"styles-module__clearing___8rM7K",confirm:"styles-module__confirm___BtMvq",markerIn:"styles-module__markerIn___x4G8D",markerConfirm:"styles-module__markerConfirm___RT4Sk",markerOut:"styles-module__markerOut___6VhQN",pending:"styles-module__pending___BiY-U",multiSelect:"styles-module__multiSelect___CPfTC",hovered:"styles-module__hovered___-mg2N",renumber:"styles-module__renumber___16lvD",renumberRoll:"styles-module__renumberRoll___akV9B",number:"styles-module__number___1JFu9",numberGlyph:"styles-module__numberGlyph___qchdk",actionGlyph:"styles-module__actionGlyph___AFRt0",actionVisible:"styles-module__actionVisible___Kb--l",plus:"styles-module__plus___xslMP"};var Po=ae(Be()),np=(0,Jn.memo)(function({annotation:t,pending:n=!1,globalIndex:o,layerIndex:r,layerSize:i,isExiting:s,isClearing:l,isAnimated:a,isNew:c,isHovered:d,isRemoving:m,onRemoveComplete:f,isEditingAny:k,renumberFrom:w,markerClickBehavior:C,onHoverEnter:S,onEnterComplete:g,onHoverLeave:x,onClick:h,onContextMenu:E}){let[O,j]=(0,Jn.useState)(a),A=(0,Jn.useRef)(n),[Y,K]=(0,Jn.useState)(!1),q=A.current&&!n&&O&&!Y;(0,Jn.useLayoutEffect)(()=>{s&&j(!1)},[s]);let Z=(0,Jn.useRef)(null),Ie=(0,Jn.useRef)({action:!1,delete:!1}),ye=d&&!k,me=ye&&C==="delete";(0,Jn.useLayoutEffect)(()=>{m||(Ie.current={action:ye,delete:me})},[m,ye,me]);let Me=m?Ie.current.action:ye,B=m?Ie.current.delete:me;as(Z,m,()=>f(t.id));let dt=t.isMultiSelect,ut=dt?"var(--agentation-color-green)":"var(--agentation-color-accent)",ue=l?Jt.clearing:s||m?Jt.exit:q?Jt.confirm:!a&&!O?Jt.enter:"",$e=l?`${Math.min(r*20,120)}ms`:m||n||q?"0ms":s?`${(i-1-r)*20}ms`:`${c?0:r*20}ms`;return(0,Po.jsxs)("button",{ref:Z,type:"button","aria-label":n?"Pending annotation":`${C==="delete"?"Delete":"Edit"} annotation ${o+1}: ${t.element}`,disabled:n||s||m||l,tabIndex:n||k?-1:0,className:`${Jt.marker} ${n?Jt.pending:""} ${dt?Jt.multiSelect:""} ${ue} ${!n&&Me?Jt.actionVisible:""} ${B?Jt.hovered:""} ${d&&!k&&!m?Jt.previewVisible:""}`,"data-annotation-marker":n?void 0:"","data-annotation-pending":n?"":void 0,style:{left:`${t.x}%`,top:t.y,backgroundColor:B?void 0:ut,animationDelay:$e},onAnimationEnd:Fe=>{Fe.target===Fe.currentTarget&&(ue===Jt.enter||ue===Jt.confirm)&&(j(!0),n||K(!0),n||g(t.id))},onMouseOver:()=>{n||S(t)},onMouseOut:Fe=>{let vt=Fe.relatedTarget;(!(vt instanceof Node)||!Fe.currentTarget.contains(vt))&&x(t.id)},onFocus:Fe=>{!n&&Fe.currentTarget.matches(":focus-visible")&&S(t)},onBlur:()=>x(t.id),onClick:Fe=>{Fe.stopPropagation(),!n&&!s&&!m&&h(t,Fe.currentTarget)},onContextMenu:E?Fe=>{C==="delete"&&(Fe.preventDefault(),Fe.stopPropagation(),!n&&!s&&!m&&E(t,Fe.currentTarget))}:void 0,children:[(0,Po.jsx)("span",{className:`${Jt.number} ${w!==null&&o>=w?Jt.renumber:""}`,"aria-hidden":"true",children:(0,Po.jsx)("span",{className:Jt.numberGlyph,children:o+1})},o),(0,Po.jsx)("span",{className:Jt.actionGlyph,"aria-hidden":"true",children:C==="delete"?(0,Po.jsx)(gy,{size:dt?18:16}):(0,Po.jsx)(xy,{size:16})}),A.current&&(0,Po.jsx)("span",{className:Jt.plus,"aria-hidden":"true",children:(0,Po.jsx)(ay,{size:12})})]})});var Mr=ae(bt());var G1=`.styles-module__switchContainer___Ka-AB {
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
}`,gu={switchContainer:"styles-module__switchContainer___Ka-AB",switchInput:"styles-module__switchInput___kYDSD",switchThumb:"styles-module__switchThumb___4sCPH"};var Gl=ae(Be()),hu=({className:e="",checked:t,onChange:n,...o})=>(0,Gl.jsxs)("div",{className:`${gu.switchContainer} ${e}`,"data-checked":t?"":void 0,children:[(0,Gl.jsx)("input",{className:gu.switchInput,checked:t,onChange:n,type:"checkbox",...o}),(0,Gl.jsx)("div",{className:gu.switchThumb})]});var Z1=ae(bt());var q1=`.styles-module__checkboxContainer___joqZk {
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
}`,ql={checkboxContainer:"styles-module__checkboxContainer___joqZk",checkboxInput:"styles-module__checkboxInput___ECzzO",checkboxCheck:"styles-module__checkboxCheck___fUXpr",checkboxCheckPath:"styles-module__checkboxCheckPath___cDyh8"};var gs=ae(Be()),K1=({className:e="",checked:t,onChange:n,...o})=>(0,gs.jsxs)("div",{className:`${ql.checkboxContainer} ${e}`,"data-checked":t?"":void 0,children:[(0,gs.jsx)("input",{className:ql.checkboxInput,type:"checkbox",checked:t,onChange:n,...o}),(0,gs.jsx)("svg",{className:ql.checkboxCheck,width:"14",height:"14",viewBox:"0 0 14 14",fill:"none",children:(0,gs.jsx)("path",{className:ql.checkboxCheckPath,d:"M3.94 7L6.13 9.19L10.5 4.81",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})})]});var J1=`.styles-module__container___w8eAF {
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
}`,op={container:"styles-module__container___w8eAF",label:"styles-module__label___J5mxE"};var hs=ae(Be()),rp=({className:e="",label:t,tooltip:n,checked:o,onChange:r,...i})=>{let s=(0,Z1.useId)();return(0,hs.jsxs)("div",{className:`${op.container} ${e}`,...i,children:[(0,hs.jsx)(K1,{id:s,onChange:r,checked:o}),(0,hs.jsx)("label",{className:op.label,htmlFor:s,children:t}),n&&(0,hs.jsx)(Lo,{content:n})]})};var e5=`@keyframes styles-module__cycleTextIn___VBNTi {
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
}`,re={settingsPanel:"styles-module__settingsPanel___qNkn-",settingsHeader:"styles-module__settingsHeader___Fn1DP",settingsBrand:"styles-module__settingsBrand___OoKlM",settingsVersion:"styles-module__settingsVersion___rXmL9",settingsSection:"styles-module__settingsSection___n5V-4",settingsLabel:"styles-module__settingsLabel___VCVOQ",cycleButton:"styles-module__cycleButton___XMBx3",cycleDot:"styles-module__cycleDot___zgSXY",dropdownButton:"styles-module__dropdownButton___mKHe8",sliderLabel:"styles-module__sliderLabel___6K5v1",slider:"styles-module__slider___v5z-c",themeToggle:"styles-module__themeToggle___3imlT",below:"styles-module__below___Vpv-k",settingsOption:"styles-module__settingsOption___JoyH-",selected:"styles-module__selected___k1-Vq",settingsPanelContainer:"styles-module__settingsPanelContainer___5it-H",settingsPage:"styles-module__settingsPage___BMn-3",slideLeft:"styles-module__slideLeft___qUvW4",automationsPage:"styles-module__automationsPage___N7By0",slideIn:"styles-module__slideIn___uXDSu",themeIconWrapper:"styles-module__themeIconWrapper___pyaYa",themeIcon:"styles-module__themeIcon___w7lAm",themeIconIn:"styles-module__themeIconIn___qUWMV",settingsSectionGrow:"styles-module__settingsSectionGrow___eZTRw",settingsRow:"styles-module__settingsRow___y-tDE",settingsRowMarginTop:"styles-module__settingsRowMarginTop___uLpGb",settingsRowDisabled:"styles-module__settingsRowDisabled___ydl3Q",cycleButtonText:"styles-module__cycleButtonText___mbbnD",cycleTextIn:"styles-module__cycleTextIn___VBNTi",cycleDots:"styles-module__cycleDots___ehp6i",active:"styles-module__active___dpAhM",colorOptions:"styles-module__colorOptions___pbxZx",colorOption:"styles-module__colorOption___Co955",settingsNavLink:"styles-module__settingsNavLink___uYIwM",settingsNavLinkRight:"styles-module__settingsNavLinkRight___XBUzC",settingsBackButton:"styles-module__settingsBackButton___fflll",automationHeader:"styles-module__automationHeader___Avra9",automationDescription:"styles-module__automationDescription___vFTmJ",learnMoreLink:"styles-module__learnMoreLink___cG7OI",autoSendContainer:"styles-module__autoSendContainer___VpkXk",autoSendLabel:"styles-module__autoSendLabel___ngNdC",disabled:"styles-module__disabled___9AZYS",mcpStatusDot:"styles-module__mcpStatusDot___8AMxP",connecting:"styles-module__connecting___QEO1r",mcpPulse:"styles-module__mcpPulse___5Q3Jj",connected:"styles-module__connected___WyFkx",disconnected:"styles-module__disconnected___mvmvQ",mcpPulseError:"styles-module__mcpPulseError___VHxhx",mcpNavIndicator:"styles-module__mcpNavIndicator___auBHI",webhookUrlInput:"styles-module__webhookUrlInput___WDDDC",checkboxField:"styles-module__checkboxField___ZrSqv",divider:"styles-module__divider___h6Yux",scaleIn:"styles-module__scaleIn___QpQ8E"};var ne=ae(Be()),t5=(0,Mr.memo)(function({settings:t,onSettingsChange:n,isDarkMode:o,onToggleTheme:r,isDevMode:i,connectionStatus:s,endpoint:l,onExited:a,isOpen:c,toolbarNearBottom:d,settingsPage:m,onSettingsPageChange:f,onHideToolbar:k}){let{ref:w}=nu(c,{keepMounted:!0,onExited:a}),C=(0,Mr.useRef)(null),S=(0,Mr.useRef)(null),g=(0,Mr.useRef)(!1);(0,Mr.useLayoutEffect)(()=>{!c||!g.current||(g.current=!1,(m==="automations"?S:C).current?.focus())},[c,m]);let x=o?"Switch to light mode":"Switch to dark mode";return(0,ne.jsx)("div",{className:`${re.settingsPanel} ${d?re.below:""}`,style:d?{bottom:"auto",top:"calc(100% + 0.5rem)"}:void 0,"data-agentation-settings-panel":!0,ref:h=>{w.current=h,h?.toggleAttribute("inert",!c)},role:"group","aria-label":"Feedback settings","aria-hidden":!c,children:(0,ne.jsxs)("div",{className:re.settingsPanelContainer,children:[(0,ne.jsxs)("div",{className:`${re.settingsPage} ${m==="automations"?re.slideLeft:""}`,ref:h=>{h?.toggleAttribute("inert",m!=="main")},"aria-hidden":m!=="main",children:[(0,ne.jsxs)("div",{className:re.settingsHeader,children:[(0,ne.jsx)("a",{className:re.settingsBrand,href:"https://agentation.com",target:"_blank",rel:"noopener noreferrer","aria-label":"Agentation",children:"Agentation"}),(0,ne.jsxs)("p",{className:re.settingsVersion,children:["v","3.1.2"]}),(0,ne.jsx)("button",{className:re.themeToggle,onClick:r,title:x,"aria-label":x,children:(0,ne.jsx)("span",{className:re.themeIconWrapper,children:(0,ne.jsx)("span",{className:re.themeIcon,children:o?(0,ne.jsx)(hy,{size:20}):(0,ne.jsx)(yy,{size:20})},o?"sun":"moon")})})]}),(0,ne.jsx)("div",{className:re.divider}),(0,ne.jsxs)("div",{className:re.settingsSection,children:[(0,ne.jsxs)("div",{className:re.settingsRow,children:[(0,ne.jsxs)("div",{className:re.settingsLabel,children:["Output Detail",(0,ne.jsx)(Lo,{content:"Controls how much detail is included in the copied output"})]}),(0,ne.jsxs)("button",{className:re.cycleButton,onClick:()=>{let E=(ms.findIndex(O=>O.value===t.outputDetail)+1)%ms.length;n({outputDetail:ms[E].value})},children:[(0,ne.jsx)("span",{className:re.cycleButtonText,children:ms.find(h=>h.value===t.outputDetail)?.label},t.outputDetail),(0,ne.jsx)("span",{className:re.cycleDots,children:ms.map(h=>(0,ne.jsx)("span",{className:`${re.cycleDot} ${t.outputDetail===h.value?re.active:""}`},h.value))})]})]}),(0,ne.jsxs)("div",{className:`${re.settingsRow} ${re.settingsRowMarginTop} ${i?"":re.settingsRowDisabled}`,children:[(0,ne.jsxs)("div",{className:re.settingsLabel,children:["React Components",(0,ne.jsx)(Lo,{content:i?"Include React component names in annotations":"Disabled \u2014 production builds minify component names, making detection unreliable. Use in development mode."})]}),(0,ne.jsx)(hu,{"aria-label":"React Components",checked:i&&t.reactEnabled,onChange:h=>n({reactEnabled:h.target.checked}),disabled:!i})]}),(0,ne.jsxs)("div",{className:`${re.settingsRow} ${re.settingsRowMarginTop}`,children:[(0,ne.jsxs)("div",{className:re.settingsLabel,children:["Hide Until Restart",(0,ne.jsx)(Lo,{content:"Hides the toolbar until you open a new tab"})]}),(0,ne.jsx)(hu,{"aria-label":"Hide Until Restart",checked:!1,onChange:h=>{h.target.checked&&k()}})]})]}),(0,ne.jsx)("div",{className:re.divider}),(0,ne.jsxs)("div",{className:re.settingsSection,children:[(0,ne.jsx)("div",{className:`${re.settingsLabel} ${re.settingsLabelMarker}`,children:"Marker Color"}),(0,ne.jsx)("div",{className:re.colorOptions,children:ys.map(h=>(0,ne.jsx)("button",{className:`${re.colorOption} ${t.annotationColorId===h.id?re.selected:""}`,style:{"--swatch":h.srgb,"--swatch-p3":h.p3},onClick:()=>n({annotationColorId:h.id}),title:h.label,"aria-label":h.label,type:"button"},h.id))})]}),(0,ne.jsx)("div",{className:re.divider}),(0,ne.jsxs)("div",{className:re.settingsSection,children:[(0,ne.jsx)(rp,{className:"checkbox-field",label:"Clear on copy/send",checked:t.autoClearAfterCopy,onChange:h=>n({autoClearAfterCopy:h.target.checked}),tooltip:"Automatically clear annotations after copying"}),(0,ne.jsx)(rp,{className:re.checkboxField,label:"Block page interactions",checked:t.blockInteractions,onChange:h=>n({blockInteractions:h.target.checked})})]}),(0,ne.jsx)("div",{className:re.divider}),(0,ne.jsxs)("button",{className:re.settingsNavLink,ref:C,onClick:h=>{g.current=h.detail===0,h.currentTarget.blur(),f("automations")},children:[(0,ne.jsx)("span",{children:"Manage MCP & Webhooks"}),(0,ne.jsxs)("span",{className:re.settingsNavLinkRight,children:[l&&s!=="disconnected"&&(0,ne.jsx)("span",{className:`${re.mcpNavIndicator} ${re[s]}`}),(0,ne.jsx)("svg",{width:"16",height:"16",viewBox:"0 0 16 16",fill:"none",xmlns:"http://www.w3.org/2000/svg",children:(0,ne.jsx)("path",{d:"M7.5 12.5L12 8L7.5 3.5",stroke:"currentColor",strokeWidth:"1.5",strokeLinecap:"round",strokeLinejoin:"round"})})]})]})]}),(0,ne.jsxs)("div",{className:`${re.settingsPage} ${re.automationsPage} ${m==="automations"?re.slideIn:""}`,ref:h=>{h?.toggleAttribute("inert",m!=="automations")},"aria-hidden":m!=="automations",children:[(0,ne.jsxs)("button",{className:re.settingsBackButton,ref:S,"aria-label":"Back to settings",onClick:h=>{g.current=h.detail===0,h.currentTarget.blur(),f("main")},children:[(0,ne.jsx)(vy,{size:16}),(0,ne.jsx)("span",{children:"Manage MCP & Webhooks"})]}),(0,ne.jsx)("div",{className:re.divider}),(0,ne.jsxs)("div",{className:re.settingsSection,children:[(0,ne.jsxs)("div",{className:re.settingsRow,children:[(0,ne.jsxs)("span",{className:re.automationHeader,children:["MCP Connection",(0,ne.jsx)(Lo,{content:"Connect via Model Context Protocol to let AI agents like Claude Code receive annotations in real-time."})]}),l&&(0,ne.jsx)("div",{className:`${re.mcpStatusDot} ${re[s]}`,title:s==="connected"?"Connected":s==="connecting"?"Connecting...":"Disconnected"})]}),(0,ne.jsxs)("p",{className:re.automationDescription,style:{paddingBottom:6},children:["MCP connection allows agents to receive and act on annotations."," ",(0,ne.jsx)("a",{href:"https://agentation.com/mcp",target:"_blank",rel:"noopener noreferrer",className:re.learnMoreLink,children:"Learn more"})]})]}),(0,ne.jsx)("div",{className:re.divider}),(0,ne.jsxs)("div",{className:`${re.settingsSection} ${re.settingsSectionGrow}`,children:[(0,ne.jsxs)("div",{className:re.settingsRow,children:[(0,ne.jsxs)("span",{className:re.automationHeader,children:["Webhooks",(0,ne.jsx)(Lo,{content:"Send annotation data to any URL endpoint when annotations change. Useful for custom integrations."})]}),(0,ne.jsxs)("div",{className:re.autoSendContainer,children:[(0,ne.jsx)("label",{htmlFor:"agentation-auto-send",className:`${re.autoSendLabel} ${t.webhooksEnabled?re.active:""} ${t.webhookUrl?"":re.disabled}`,children:"Auto-Send"}),(0,ne.jsx)(hu,{id:"agentation-auto-send",checked:t.webhooksEnabled,onChange:h=>n({webhooksEnabled:h.target.checked}),disabled:!t.webhookUrl})]})]}),(0,ne.jsx)("p",{className:re.automationDescription,children:"The webhook URL will receive live annotation changes and annotation data."}),(0,ne.jsx)("textarea",{className:re.webhookUrlInput,placeholder:"Webhook URL","aria-label":"Webhook URL",value:t.webhookUrl,onKeyDown:h=>h.stopPropagation(),onChange:h=>n({webhookUrl:h.target.value})})]})]})]})})});var yu=ae(bt());var Kl=ae(Be());function n5({x:e,y:t,elementName:n,reactComponents:o}){let r=(0,yu.useRef)(null);return(0,yu.useLayoutEffect)(()=>{let i=r.current;if(!i)return;let s=()=>{let l=i.offsetWidth,a=i.offsetHeight;i.style.left=`${Math.max(8,Math.min(e,window.innerWidth-l-8))}px`;let c=t-a-8;i.style.top=`${Math.max(8,Math.min(c,window.innerHeight-a-8))}px`};return s(),window.addEventListener("resize",s),()=>window.removeEventListener("resize",s)},[e,t,n,o]),(0,Kl.jsxs)("div",{ref:r,className:`${H.hoverTooltip} ${H.enter}`,children:[o&&(0,Kl.jsx)("div",{className:H.hoverReactPath,children:o}),(0,Kl.jsx)("div",{className:H.hoverElementName,children:n})]})}var o5=`@charset "UTF-8";
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
}`;var V=ae(Be()),l5=ae(bt()),Dk=[o5,Cy,Jc,q1,Ty,Sy,us,Q1,Y1,J1,e5,G1].join(`
`);function xu(e,t="filtered",n){let{name:o,path:r}=kr(e,n);if(t==="off")return{name:o,elementName:o,path:r,reactComponents:null};let i=m1(e,{mode:t});return{name:i.path?`${i.path} ${o}`:o,elementName:o,path:r,reactComponents:i.path}}var r5=!1,ip={outputDetail:"standard",autoClearAfterCopy:!1,annotationColorId:"blue",blockInteractions:!0,reactEnabled:!0,markerClickBehavior:"edit",webhookUrl:"",webhooksEnabled:!0},i5=e=>{if(!e||!e.trim())return!1;try{let t=new URL(e.trim());return t.protocol==="http:"||t.protocol==="https:"}catch{return!1}},Ak={compact:"off",standard:"filtered",detailed:"smart",forensic:"all"},Tr=e=>e.metaKey||e.ctrlKey,ys=[{id:"indigo",label:"Indigo",srgb:"#6155F5",p3:"color(display-p3 0.38 0.33 0.96)"},{id:"blue",label:"Blue",srgb:"#0088FF",p3:"color(display-p3 0.00 0.53 1.00)"},{id:"cyan",label:"Cyan",srgb:"#00C3D0",p3:"color(display-p3 0.00 0.76 0.82)"},{id:"green",label:"Green",srgb:"#34C759",p3:"color(display-p3 0.20 0.78 0.35)"},{id:"yellow",label:"Yellow",srgb:"#FFCC00",p3:"color(display-p3 1.00 0.80 0.00)"},{id:"orange",label:"Orange",srgb:"#FF8D28",p3:"color(display-p3 1.00 0.55 0.16)"},{id:"red",label:"Red",srgb:"#FF383C",p3:"color(display-p3 1.00 0.22 0.24)"}],$k=[...ys.map(e=>`
    [data-agentation-accent="${e.id}"] {
      --agentation-color-accent: ${e.srgb};
    }
    @supports (color: color(display-p3 0 0 0)) {
      [data-agentation-accent="${e.id}"] {
        --agentation-color-accent: ${e.p3};
      }
    }
  `),`:host {
    ${ys.map(e=>`--agentation-color-${e.id}: ${e.srgb};`).join(`
`)}
  }`,`@supports (color: color(display-p3 0 0 0)) {
    :host {
      ${ys.map(e=>`--agentation-color-${e.id}: ${e.p3};`).join(`
`)}
    }
  }`].join("");function sp(e){let t=e;for(let o=wn(t.ownerDocument);o;o=wn(t.ownerDocument))t=o;let n=t;for(;n&&n!==document.body;){let r=window.getComputedStyle(n).position;if(r==="fixed"||r==="sticky")return!0;n=n.parentElement}return!1}function Jl(e){return e.kind!=="placement"&&e.kind!=="rearrange"&&e.status!=="resolved"&&e.status!=="dismissed"}function vu(e){let t=du(e),n=t.found?t:f1(e);if(n.found&&n.source)return p1(n.source,"path")}function lp(e={}){let t=G0(e.useHashLocation??!1),n=(0,M.useState)(e.defaultActive??!1),o=Z0(e.portalContainer);return o?(0,s5.createPortal)((0,l5.createElement)(Fk,{...e,key:e.useHashLocation?t:void 0,pathname:t,activeState:n,portalHost:o}),o):null}function Fk({pathname:e,activeState:t,portalHost:n,useHashLocation:o=!1,appName:r,enableKeyboardShortcuts:i=!0,identifyingAttributes:s=Qc,copyFormat:l="markdown",onOpenSource:a,portalContainer:c,demoAnnotations:d,demoDelay:m=1e3,enableDemoMode:f=!1,onAnnotationAdd:k,onAnnotationDelete:w,onAnnotationUpdate:C,onAnnotationsClear:S,onCopy:g,onSubmit:x,copyToClipboard:h=!0,endpoint:E,sessionId:O,onSessionCreated:j,webhookUrl:A,className:Y,captureScreenshots:K=!0,screenshotReference:q,toolbar:Z="floating",theme:Ie,onReady:ye,onStateChange:me,onAnnotationsChange:Me}){let[B,dt]=t,[,ut]=(0,M.useState)(0),[ue]=(0,M.useState)(()=>W0(document,()=>ut(p=>p+1)));(0,M.useEffect)(()=>(ue.start(),()=>ue.stop()),[ue]);let $e=typeof l=="object"?l.attribute:void 0,Fe=(0,M.useMemo)(()=>$e?[...s,$e]:s,[s,$e]),vt=(0,M.useRef)(!0);(0,M.useLayoutEffect)(()=>(vt.current=!0,()=>{vt.current=!1}),[]);let Ct=(0,M.useCallback)(p=>o?q0(JSON.stringify([E,e]),p):p(),[E,e,o]),Ht=(0,M.useRef)(new Map),Zt=(0,M.useRef)(new Set),ci=p=>Jl(p)&&!Zt.current.has(p.id),mo=async(...p)=>{let y=await Ym(...p),b=p[2],T=b.id;if(T&&(Ht.current.set(T,y.id),Zt.current.has(T)&&Zt.current.add(y.id)),!o){let N=new URL(b.url||window.location.href).pathname,P=Go(N).find(I=>I.id===T);try{if(Zt.current.has(T))await zl(p[0],y.id);else if(P&&P.comment!==b.comment)return await cu(p[0],y.id,{comment:P.comment}),{...y,comment:P.comment}}catch(I){console.warn("[Agentation] Failed to apply changes made during sync:",I)}}return y},qo=p=>o?{...p,annotations:p.annotations.filter(y=>!Zt.current.has(y.id)&&K0(y.url||p.url,e,window.location.origin))}:p,po=(0,M.useRef)(e);po.current=e;let fo=(p,y,b,T=e)=>{let N=F0(p,Go(T),y,Ht.current).filter(ci);T===po.current&&vt.current&&en(N),su(T,N,b)},[ze,en]=(0,M.useState)([]),[go,Zn]=(0,M.useState)(!0),[Lr,No]=(0,M.useState)(()=>c1()),[Ko,G]=(0,M.useState)(!1);(0,M.useLayoutEffect)(()=>{xm()},[]);let ie=(0,M.useRef)(null),xe=(0,M.useRef)(null),De=(0,M.useRef)(null),Se=(0,M.useRef)(null),gt=(0,M.useRef)(!1),rt=(0,M.useRef)(!1),Ge=(0,M.useRef)(!1);(0,M.useLayoutEffect)(()=>{B&&gt.current?(gt.current=!1,De.current?.querySelector("button:not(:disabled)")?.focus()):!B&&rt.current&&(rt.current=!1,xe.current?.focus())},[B]),(0,M.useEffect)(()=>{let p=b=>{let T=ie.current;T&&b.composedPath().includes(T)&&b.stopPropagation()},y=["mousedown","click","pointerdown"];return y.forEach(b=>n.addEventListener(b,p)),()=>{y.forEach(b=>n.removeEventListener(b,p))}},[n]);let[Ye,Lt]=(0,M.useState)(!1),[L,D]=(0,M.useState)(!1),[z,U]=(0,M.useState)(null),[se,ce]=(0,M.useState)({x:0,y:0}),[R,le]=(0,M.useState)(null),[we,He]=(0,M.useState)(!1),Je=wm(),it=wm(),[We,Ze]=(0,M.useState)("idle"),[Te,wt]=(0,M.useState)(!1),ht=(0,M.useRef)(new Set),at=(0,M.useRef)(new Set),Ae=(0,M.useRef)(),Ve=(0,M.useCallback)(()=>{!ht.current.size&&!Ae.current&&wt(!1)},[]);(0,M.useEffect)(()=>()=>clearTimeout(Ae.current),[]);let[et,ft]=(0,M.useState)(null),[Wt,sn]=(0,M.useState)(null),[ho,Xt]=(0,M.useState)([]),[Io,ui]=(0,M.useState)(null),Rn=(0,M.useRef)(null);(0,M.useEffect)(()=>()=>{Rn.current&&clearTimeout(Rn.current)},[]);let[pe,Rr]=(0,M.useState)(null),di=(0,M.useRef)(null),wu=(0,M.useRef)(!1),[bu,xs]=(0,M.useState)(!1);(0,M.useLayoutEffect)(()=>{if(pe||!di.current)return;let p=di.current;if(di.current=null,R)return;(B&&p.isConnected&&!p.disabled?p:xe.current)?.focus({preventScroll:!0})},[pe,B,R]);let[ku,Pr]=(0,M.useState)(null),[ap,vs]=(0,M.useState)([]),[Nr,cp]=(0,M.useState)(0),[up,dp]=(0,M.useState)(!1),[Xe,u5]=(0,M.useState)(!1),[Pn,_p]=(0,M.useState)(!1),[ln,Ir]=(0,M.useState)(!1),[d5,mp]=(0,M.useState)("main"),[pp,Su]=(0,M.useState)(!1),[Qe,Cu]=(0,M.useState)(!1),[Dr,ws]=(0,M.useState)(!1),[Pe,yo]=(0,M.useState)([]),[_i,Ar]=(0,M.useState)(null),Eu=(0,M.useRef)(!1),[yt,fp]=(0,M.useState)(!1),[gp,Mu]=(0,M.useState)(!1),[hp,_5]=(0,M.useState)(1),[yp,zk]=(0,M.useState)("new-page"),[an,Zl]=(0,M.useState)(""),[m5,p5]=(0,M.useState)(!1),[_e,eo]=(0,M.useState)(null),Tu=(0,M.useRef)(!1),Lu=(0,M.useRef)({rearrange:null,placements:[]}),$r=(0,M.useRef)({rearrange:null,placements:[]}),[f5,xp]=(0,M.useState)(0),[g5,h5]=(0,M.useState)(0),[mi,vp]=(0,M.useState)([]),[pi,wp]=(0,M.useState)(null),bp=(0,M.useRef)({designPlacements:Pe,rearrangeState:_e,blankCanvas:yt,wireframePurpose:an});bp.current={designPlacements:Pe,rearrangeState:_e,blankCanvas:yt,wireframePurpose:an};let fi=(0,M.useRef)({placements:mi,rearrange:pi}),bs=(0,M.useRef)(new Set),ea=(0,M.useRef)(new Set),to=(0,M.useRef)(null),gi=(0,M.useRef)(),kp=Qe&&B&&!Dr&&yt;(0,M.useEffect)(()=>{if(kp){Mu(!1);let p=Zi(()=>{Mu(!0)});return()=>cancelAnimationFrame(p)}else Mu(!1)},[kp]);let Ru=(0,M.useRef)(new Map),Pu=(0,M.useRef)([]),Nu=(0,M.useRef)(new Map),Fr=(0,M.useRef)(null),[Nn,ta]=(0,M.useState)(!1),[In,y5]=(0,M.useState)([]),Iu=(0,M.useRef)(In);Iu.current=In;let[Sp,Hk]=(0,M.useState)(null),Du=(0,M.useRef)(null),Wk=(0,M.useRef)(!1),Uk=(0,M.useRef)([]),jk=(0,M.useRef)(0),Yk=(0,M.useRef)(null),Vk=(0,M.useRef)(null),Xk=(0,M.useRef)(1),[Au,Cp]=(0,M.useState)(!1),hi=(0,M.useRef)(null),[kn,Jo]=(0,M.useState)([]),ks=(0,M.useRef)(!1),cn=()=>{Su(!0)},x5=()=>{Su(!1)},Ep=()=>{Au||(hi.current=Ne(()=>Cp(!0),850))},Mp=()=>{hi.current&&(clearTimeout(hi.current),hi.current=null),Cp(!1),x5()};(0,M.useEffect)(()=>()=>{hi.current&&clearTimeout(hi.current)},[]);let[tt,v5]=(0,M.useState)(()=>{try{let p=JSON.parse(localStorage.getItem("feedback-toolbar-settings")??"");return{...ip,...p,annotationColorId:ys.find(y=>y.id===p.annotationColorId)?p.annotationColorId:ip.annotationColorId}}catch{return ip}}),[xo,$u]=(0,M.useState)(!0),[Tp,Lp]=(0,M.useState)(!1),Rp=(0,M.useCallback)(p=>{v5(y=>({...y,...p}))},[]),w5=(0,M.useCallback)(()=>{ie.current?.classList.add(H.disableTransitions),$u(p=>!p),Zi(()=>{ie.current?.classList.remove(H.disableTransitions)})},[]),Pp=!1,Do=Pp&&tt.reactEnabled?Ak[tt.outputDetail]:"off",[Ut,Fu]=(0,M.useState)(o?null:O??null),Np=(0,M.useRef)(!1),[Or,Br]=(0,M.useState)(E?"connecting":"disconnected"),[Dt,Ou]=(0,M.useState)(null),[na,Ip]=(0,M.useState)(!1),Ss=(0,M.useRef)(null),oa=(0,M.useRef)(!1),zr=(0,M.useRef)(new Set),ra=(0,M.useRef)(new Map),Dp=(0,M.useCallback)(p=>{zr.current.add(p),no.current===p&&(no.current=null)},[]),[Zo,ia]=(0,M.useState)(new Set),[Wn,Cs]=(0,M.useState)(!1),[Hr,yi]=(0,M.useState)(!1),[Ao,Bu]=(0,M.useState)(!1),Wr=(0,M.useRef)(null),Un=(0,M.useRef)(null),Es=(0,M.useRef)(null),xi=(0,M.useRef)(null),Ms=(0,M.useRef)(!1),Ap=(0,M.useRef)(0),no=(0,M.useRef)(null),$p=(0,M.useRef)(null),zu=8,b5=50,Hu=(0,M.useRef)(null),sa=(0,M.useRef)(null),Ts=(0,M.useRef)(null),k5=(0,M.useCallback)(()=>mp("main"),[]);(0,M.useEffect)(()=>{ln||Su(!1)},[ln]),(0,M.useLayoutEffect)(()=>{ln&&Ge.current&&(Ge.current=!1,ie.current?.querySelector("[data-agentation-settings-panel] button")?.focus())},[ln]);let la=B&&go&&!Qe;(0,M.useEffect)(()=>{if(la)D(!1),Lt(!0),zr.current.clear();else if(Ye){D(!0);let p=Ne(()=>{Lt(!1),D(!1)},250);return()=>clearTimeout(p)}},[la]),(0,M.useEffect)(()=>{u5(!0),cp(window.scrollY);let p=Go(e);en(p.filter(Jl)),r5||(Lp(!0),r5=!0,Ne(()=>Lp(!1),750));try{let y=Ie?null:localStorage.getItem("feedback-toolbar-theme");y!==null&&$u(y==="dark")}catch{}try{let y=localStorage.getItem("feedback-toolbar-position");if(y){let b=JSON.parse(y);typeof b.x=="number"&&typeof b.y=="number"&&Ou(b)}}catch{}},[e]),(0,M.useEffect)(()=>{Xe&&localStorage.setItem("feedback-toolbar-settings",JSON.stringify(tt))},[tt,Xe]),(0,M.useEffect)(()=>{Ie&&$u(Ie==="dark")},[Ie]),(0,M.useEffect)(()=>{Xe&&!Ie&&localStorage.setItem("feedback-toolbar-theme",xo?"dark":"light")},[xo,Xe]);let Fp=(0,M.useRef)(!1);(0,M.useEffect)(()=>{let p=Fp.current;Fp.current=na,p&&!na&&Dt&&Xe&&localStorage.setItem("feedback-toolbar-position",JSON.stringify(Dt))},[na,Dt,Xe]),(0,M.useEffect)(()=>{if(!E||!Xe||Np.current)return;Np.current=!0,Br("connecting");let p=window.location.href;Ct(async()=>{try{let b=l1(e),T=O||b,N=!1;if(T)try{let P=Go(e),I=qo(await jm(E,T));Pu.current=I.annotations.filter(fe=>fe.kind==="placement"||fe.kind==="rearrange"),vt.current&&(Fu(I.id),Br("connected")),lu(e,I.id),N=!0;let X=Go(e).filter(Jl),J=new Set(I.annotations.map(fe=>fe.id)),ge=X.filter(fe=>!J.has(fe.id));if(ge.length>0){let he=`${typeof window<"u"?window.location.origin:""}${e}`,Oe=(await Promise.allSettled(ge.map(ve=>mo(E,I.id,{...ve,sessionId:I.id,url:he})))).map((ve,Ce)=>ve.status==="fulfilled"?ve.value:(console.warn("[Agentation] Failed to sync annotation:",ve.reason),ge[Ce])),de=[...I.annotations,...Oe];fo(P,de,I.id)}else fo(P,I.annotations,I.id)}catch(P){console.warn("[Agentation] Could not join session, creating new:",P),a1(e)}if(!N){let P=await au(E,p);lu(e,P.id),vt.current&&(Fu(P.id),Br("connected"),j?.(P.id));let I=o?new Map([[e,Go(e)]]):Jy(),X=typeof window<"u"?window.location.origin:"",J=[];for(let[ge,fe]of I){let he=fe.filter(de=>Jl(de)&&!de._syncedTo);if(he.length===0)continue;let Le=`${X}${ge}`,Oe=ge===e;J.push((async()=>{try{let de=Oe?P:await au(E,Le),Ce=(await Promise.allSettled(he.map(nt=>mo(E,de.id,{...nt,sessionId:de.id,url:Le})))).map((nt,Et)=>nt.status==="fulfilled"?nt.value:(console.warn("[Agentation] Failed to sync annotation:",nt.reason),he[Et]));fo(he,Ce,de.id,ge)}catch(de){console.warn(`[Agentation] Failed to sync annotations for ${ge}:`,de)}})())}await Promise.allSettled(J)}}catch(b){vt.current&&Br("disconnected"),console.warn("[Agentation] Failed to initialize session, using local storage:",b)}})},[E,O,Xe,j,e,Ct]),(0,M.useEffect)(()=>{if(!E||!Xe)return;let p=async()=>{try{(await fetch(`${E}/health`)).ok?Br("connected"):Br("disconnected")}catch{Br("disconnected")}};p();let y=Yc(p,1e4);return()=>clearInterval(y)},[E,Xe]);let aa=(0,M.useRef)(ze),Op=(0,M.useRef)(!1);(0,M.useLayoutEffect)(()=>{aa.current=ze,Op.current=ze.length>0||Pe.length>0||(_e?.sections.length??0)>0},[ze,Pe.length,_e?.sections.length]);let Wu=(0,M.useCallback)(p=>{let y=ht.current.has(p);if(y&&(at.current.delete(p),at.current.size))return;let b=y?new Set(ht.current):new Set([p]);y&&(ht.current.clear(),Ve());for(let P of b)ra.current.delete(P),zr.current.delete(P);en(P=>P.filter(I=>!b.has(I.id))),ia(P=>new Set([...P].filter(I=>!b.has(I))));let T=y?[]:aa.current.filter(P=>P.kind!=="placement"&&P.kind!=="rearrange"),N=T.findIndex(P=>P.id===p);N>=0&&N<T.length-1&&(ui(P=>P===null?N:Math.min(P,N)),Rn.current&&clearTimeout(Rn.current),Rn.current=Ne(()=>ui(null),200))},[Ve]);(0,M.useEffect)(()=>!E||!Xe||!Ut?void 0:j1(E,Ut,()=>Op.current,b=>{let{id:T,kind:N}=b;if(N==="placement"){for(let[P,I]of Ru.current)if(I===T){Fr.current?.placements.forget(P),yo(X=>X.filter(J=>J.id!==P));break}}else if(N==="rearrange"){for(let[P,I]of Nu.current)if(I===T){Fr.current?.rearrange.forget(P),eo(X=>{if(!X)return null;let J=X.sections.filter(ge=>ge.id!==P);return J.length===0?null:{...X,sections:J}});break}}else{if(!aa.current.some(P=>P.id===T))return;ia(P=>new Set(P).add(T))}}),[E,Xe,Ut]),(0,M.useEffect)(()=>{if(!E||!Xe)return;let p=$p.current==="disconnected",y=Or==="connected";$p.current=Or,p&&y&&Ct(async()=>{try{let T=Go(e).filter(Jl);if(T.length===0)return;let P=`${typeof window<"u"?window.location.origin:""}${e}`,I=Ut,X=[];if(I)try{X=qo(await jm(E,I)).annotations}catch{I=null}I||(I=(await au(E,P)).id,vt.current&&Fu(I),lu(e,I));let J=new Set(X.map(fe=>fe.id)),ge=T.filter(fe=>!J.has(fe.id));if(ge.length>0){let he=(await Promise.allSettled(ge.map(Oe=>mo(E,I,{...Oe,sessionId:I,url:P})))).map((Oe,de)=>Oe.status==="fulfilled"?Oe.value:(console.warn("[Agentation] Failed to sync annotation on reconnect:",Oe.reason),ge[de])),Le=[...X,...he];fo(T,Le,I)}}catch(T){console.warn("[Agentation] Failed to sync on reconnect:",T)}})},[Or,E,Xe,Ut,e,Ct]);let S5=(0,M.useCallback)(()=>{Ko||(G(!0),Ir(!1),dt(!1),Ne(()=>{u1(!0),No(!0),G(!1)},400))},[Ko]);(0,M.useEffect)(()=>{if(!f||!Xe||!d||d.length===0||ze.length>0)return;let p=[];return p.push(Ne(()=>{dt(!0)},m-200)),d.forEach((y,b)=>{let T=m+b*300;p.push(Ne(()=>{let N=document.querySelector(y.selector);if(!N)return;let P=Ft(N),{name:I,path:X}=kr(N),J={id:`demo-${Date.now()}-${b}`,x:(P.left+P.width/2)/window.innerWidth*100,y:P.top+P.height/2+window.scrollY,comment:y.comment,element:I,elementPath:X,timestamp:Date.now(),selectedText:y.selectedText,boundingBox:{x:P.left,y:P.top+window.scrollY,width:P.width,height:P.height},nearbyText:os(N),cssClasses:rs(N)};en(ge=>[...ge,J])},T))}),()=>{p.forEach(clearTimeout)}},[f,Xe,d,m]),(0,M.useEffect)(()=>{let p=()=>{cp(window.scrollY),ut(y=>y+1),dp(!0),Ts.current&&clearTimeout(Ts.current),Ts.current=Ne(()=>{dp(!1)},150)};return ue.addEventListener("scroll",p,{passive:!0,capture:!0}),()=>{ue.removeEventListener("scroll",p,!0),Ts.current&&clearTimeout(Ts.current)}},[ue]),(0,M.useEffect)(()=>{if(!Xe)return;let p=ze.filter(y=>!Zo.has(y.id));p.length>0?Ut?su(e,p,Ut):Om(e,p):localStorage.removeItem(Ol(e))},[ze,e,Xe,Ut,Te,Zo]),(0,M.useEffect)(()=>{if(Xe&&!Eu.current){Eu.current=!0;let p=Zy(e);p.length>0&&yo(p)}},[Xe,e]),(0,M.useEffect)(()=>{if(Xe&&Eu.current&&!yt){let p=Pe.filter(y=>!mi.includes(y));p.length>0?e1(e,p):t1(e)}},[Pe,e,Xe,yt,mi]),(0,M.useEffect)(()=>{if(Xe&&!Tu.current){Tu.current=!0;let p=n1(e);if(p){let y={...p,sections:p.sections.map(b=>({...b,currentRect:b.currentRect??{...b.originalRect}}))};eo(y)}}},[Xe,e]),(0,M.useEffect)(()=>{Xe&&Tu.current&&!yt&&(_e&&_e!==pi?o1(e,_e):r1(e))},[_e,e,Xe,yt,pi]);let Uu=(0,M.useRef)(!1);(0,M.useEffect)(()=>{if(Xe&&!Uu.current){Uu.current=!0;let p=i1(e);p&&($r.current={rearrange:p.rearrange,placements:p.placements||[]},p.purpose&&Zl(p.purpose))}},[Xe,e]),(0,M.useEffect)(()=>{if(!Xe||!Uu.current||Te)return;let p=$r.current;yt?(_e?.sections?.length??0)>0||Pe.length>0||an?Wm(e,{rearrange:_e,placements:Pe,purpose:an}):Bl(e):(p.rearrange?.sections?.length??0)>0||p.placements.length>0||an?Wm(e,{rearrange:p.rearrange,placements:p.placements,purpose:an}):Bl(e)},[_e,Pe,an,yt,e,Xe,Te]),(0,M.useEffect)(()=>{Qe&&!_e&&eo({sections:[],originalOrder:[],detectedAt:Date.now()})},[Qe,_e]),(0,M.useEffect)(()=>{if(!E||!Ut)return;let p={create:b=>Ct(()=>Ym(E,Ut,b)),update:(b,T)=>Ct(()=>cu(E,b,T)),remove:b=>Ct(()=>zl(E,b))};Ru.current=new Map,Nu.current=new Map;let y={placements:ep(p,Ru.current,Pu.current.filter(b=>b.kind==="placement")),rearrange:ep(p,Nu.current,Pu.current.filter(b=>b.kind==="rearrange"))};return Fr.current=y,()=>{y.placements.dispose(),y.rearrange.dispose(),Fr.current===y&&(Fr.current=null)}},[E,Ut,e,Ct]),(0,M.useEffect)(()=>{let p=window.location.pathname+window.location.search+window.location.hash;Fr.current?.placements.replace(Pe.filter(y=>!mi.includes(y)).map(y=>({id:y.id,x:y.x/window.innerWidth*100,y:y.y,comment:`Place ${y.type} at (${Math.round(y.x)}, ${Math.round(y.y)}), ${y.width}\xD7${y.height}px${y.text?` \u2014 "${y.text}"`:""}`,element:`[design:${y.type}]`,elementPath:"[placement]",timestamp:y.timestamp,url:p,intent:"change",severity:"important",kind:"placement",placement:{componentType:y.type,width:y.width,height:y.height,scrollY:y.scrollY,text:y.text}})))},[Pe,E,Ut,e,mi]),(0,M.useEffect)(()=>{let p=Fr.current;if(!p)return;if(_e===pi){p.rearrange.replace([]);return}let y=Ne(()=>{let b=window.location.pathname+window.location.search+window.location.hash,T=[];for(let N of _e?.sections??[]){let P=N.originalRect,I=N.currentRect,X=Math.abs(P.x-I.x)>1||Math.abs(P.y-I.y)>1||Math.abs(P.width-I.width)>1||Math.abs(P.height-I.height)>1;if(!X&&!N.note)continue;let J=N.note?` \u2014 "${N.note}"`:"";T.push({id:N.id,x:I.x/window.innerWidth*100,y:I.y,comment:X?`Move ${N.label} section (${N.tagName}) \u2014 from (${Math.round(P.x)},${Math.round(P.y)}) ${Math.round(P.width)}\xD7${Math.round(P.height)} to (${Math.round(I.x)},${Math.round(I.y)}) ${Math.round(I.width)}\xD7${Math.round(I.height)}${J}`:`Note on ${N.label} section (${N.tagName})${J}`,element:N.selector,elementPath:"[rearrange]",timestamp:_e.detectedAt,url:b,intent:"change",severity:"important",kind:"rearrange",rearrange:{selector:N.selector,label:N.label,tagName:N.tagName,originalRect:P,currentRect:I}})}p.rearrange.replace(T)},300);return()=>clearTimeout(y)},[_e,E,Ut,e,pi]);let ca=(0,M.useCallback)(()=>{clearTimeout(gi.current),ws(!1),Cu(!0)},[]);(0,M.useEffect)(()=>()=>clearTimeout(gi.current),[]);let vi=(0,M.useCallback)(()=>{ws(!0),Cu(!1),Ar(null),clearTimeout(gi.current),gi.current=Ne(()=>{ws(!1)},300)},[]),Ls=(0,M.useCallback)(()=>{let p=xe.current?.getRootNode();rt.current=!!p?.activeElement&&!!ie.current?.contains(p.activeElement),rt.current&&p?.activeElement?.blur(),Ir(!1),Qe&&(ws(!0),Cu(!1),Ar(null),clearTimeout(gi.current),gi.current=Ne(()=>{ws(!1)},300)),dt(!1)},[Qe]),Bp=(0,M.useCallback)(()=>{Pn||($0(),_p(!0))},[Pn]),ua=(0,M.useCallback)(()=>{Pn&&(vm(),_p(!1))},[Pn]),da=(0,M.useCallback)(()=>{Pn?ua():Bp()},[Pn,Bp,ua]),Rs=(0,M.useCallback)((p=kn)=>{let y=p.filter(I=>I.element.isConnected);if(y.length===0){Jo([]);return}let b=y[0],T=b.element,N=y.length>1,P=y.map(I=>Ft(I.element));if(N){let I={left:Math.min(...P.map(Ce=>Ce.left)),top:Math.min(...P.map(Ce=>Ce.top)),right:Math.max(...P.map(Ce=>Ce.right)),bottom:Math.max(...P.map(Ce=>Ce.bottom))},X=y.slice(0,5).map(Ce=>Ce.name).join(", "),J=y.length>5?` +${y.length-5} more`:"",ge=P.map(Ce=>({x:Ce.left,y:Ce.top+window.scrollY,width:Ce.width,height:Ce.height})),he=y[y.length-1].element,Le=P[P.length-1],Oe=Le.left+Le.width/2,de=Le.top+Le.height/2,ve=sp(he);le({id:Date.now().toString(),x:Oe/window.innerWidth*100,y:ve?de:de+window.scrollY,clientY:de,element:`${y.length} elements: ${X}${J}`,elementPath:"multi-select",boundingBox:{x:I.left,y:I.top+window.scrollY,width:I.right-I.left,height:I.bottom-I.top},isMultiSelect:!0,isFixed:ve,elementBoundingBoxes:ge,multiSelectElements:y.map(Ce=>Ce.element),targetElement:he,fullPath:is(T),accessibility:Il(T),computedStyles:Nl(T),computedStylesObj:Pl(T),nearbyElements:Rl(T),cssClasses:rs(T),nearbyText:os(T),sourceFile:vu(T),attributes:ts(T,Fe)})}else{let I=P[0],X=sp(T);le({id:Date.now().toString(),x:I.left/window.innerWidth*100,y:X?I.top:I.top+window.scrollY,clientY:I.top,element:b.name,elementPath:b.path,boundingBox:{x:I.left,y:X?I.top:I.top+window.scrollY,width:I.width,height:I.height},isFixed:X,fullPath:is(T),accessibility:Il(T),computedStyles:Nl(T),computedStylesObj:Pl(T),nearbyElements:Rl(T),cssClasses:rs(T),nearbyText:os(T),reactComponents:b.reactComponents,targetElement:T,sourceFile:vu(T),attributes:ts(T,Fe)})}Jo([]),U(null)},[kn,Fe]);(0,M.useEffect)(()=>{B||(le(null),Rr(null),Pr(null),vs([]),U(null),Ir(!1),Jo([]),ks.current=!1,Pn&&ua())},[B,Pn,ua]),(0,M.useEffect)(()=>()=>{vm()},[]),(0,M.useEffect)(()=>{if(!B)return;let p=["p","span","h1","h2","h3","h4","h5","h6","li","td","th","label","blockquote","figcaption","caption","legend","dt","dd","pre","code","em","strong","b","i","u","s","a","time","address","cite","q","abbr","dfn","mark","small","sub","sup","[contenteditable]"].join(", "),y=document.createElement("style");return y.id="agentation-cursor",y.textContent=`
      body { cursor: crosshair !important; }
      body :is(${p}) { cursor: text !important; }
    `,document.head.appendChild(y),()=>{let b=document.getElementById("agentation-cursor");b&&b.remove()}},[B]),(0,M.useEffect)(()=>{if(Sp!==null&&B)return document.documentElement.setAttribute("data-drawing-hover",""),()=>document.documentElement.removeAttribute("data-drawing-hover")},[Sp,B]),(0,M.useEffect)(()=>{if(!B||R||pe||Nn||Qe)return;let p=null,y=(P,I,X)=>{let J=Dl(P,I),ge=X?bm(P,I):J;if(!ge||Vt(ge,"[data-feedback-toolbar], [data-annotation-popup], [data-annotation-marker]")){U(null);return}let{name:fe,elementName:he,path:Le,reactComponents:Oe}=xu(ge,Do,Fe);U({element:fe,elementName:he,elementPath:Le,rect:Ft(ge),reactComponents:Oe,isPiercing:X&&ge!==J}),ce({x:P,y:I})},b=P=>{let I=P.composedPath()[0]||P.target;if(Vt(I,"[data-feedback-toolbar], [data-annotation-popup], [data-annotation-marker]")){p=null,U(null);return}p={x:P.clientX,y:P.clientY},y(P.clientX,P.clientY,Tr(P))},T=P=>{(P.key==="Meta"||P.key==="Control")&&p&&y(p.x,p.y,Tr(P))},N=()=>{p=null,U(null)};return ue.addEventListener("mousemove",b),ue.addEventListener("keydown",T),ue.addEventListener("keyup",T),ue.addEventListener("mouseleave",N),window.addEventListener("blur",N),()=>{ue.removeEventListener("mousemove",b),ue.removeEventListener("keydown",T),ue.removeEventListener("keyup",T),ue.removeEventListener("mouseleave",N),window.removeEventListener("blur",N)}},[B,R,pe,Nn,Qe,Do,Fe]);let _a=(0,M.useCallback)((p,y)=>{if(pe&&!Hr){sa.current?.shake();return}if(R&&!Wn){if(ie.current?.querySelector("[data-annotation-popup]:not([data-annotation-card]) textarea")?.value.trim()){Hu.current?.shake();return}Cs(!0)}if(di.current=y??null,wu.current=y?.matches(":focus-visible")??!1,xs(!1),yi(!1),Rr(p),ft(null),sn(null),Xt([]),p.elementBoundingBoxes?.length){let b=[];for(let T of p.elementBoundingBoxes){let N=T.x+T.width/2,P=T.y+T.height/2-window.scrollY,I=Al(N,P,T);I&&b.push(I)}vs(b),Pr(null)}else if(p.boundingBox){let b=p.boundingBox,T=b.x+b.width/2,N=p.isFixed?b.y+b.height/2:b.y+b.height/2-window.scrollY,P=Al(T,N,b);if(P){let I=Ft(P),X=I.width/b.width,J=I.height/b.height;X<.5||J<.5?Pr(null):Pr(P)}else Pr(null);vs([])}else Pr(null),vs([])},[R,Wn,pe,Hr]);(0,M.useEffect)(()=>{if(!B||Nn||Qe)return;let p=y=>{if(Ms.current){Ms.current=!1,y.preventDefault(),y.stopPropagation();return}let b=y.composedPath()[0]||y.target;if(Vt(b,"[data-feedback-toolbar]")||Vt(b,"[data-annotation-popup]")||Vt(b,"[data-annotation-marker]"))return;if(Tr(y)&&!R&&!pe){y.preventDefault(),y.stopPropagation(),ks.current=y.shiftKey;let Ce=bm(y.clientX,y.clientY);if(!Ce)return;let nt=Ft(Ce),{name:Et,path:gn,reactComponents:Ee}=xu(Ce,Do,Fe),be=kn.findIndex(st=>st.element===Ce);be>=0?Jo(st=>st.filter((At,wo)=>wo!==be)):Jo(st=>[...st,{element:Ce,rect:nt,name:Et,path:gn,reactComponents:Ee??void 0}]);return}let T=Vt(b,"button, a, input, select, textarea, [role='button'], [onclick]");if(tt.blockInteractions&&(y.preventDefault(),y.stopPropagation()),R&&!Wn){if(T&&!tt.blockInteractions)return;y.preventDefault(),Hu.current?.shake();return}if(pe&&!Hr){if(T&&!tt.blockInteractions)return;y.preventDefault(),sa.current?.shake();return}y.preventDefault();let N=Dl(y.clientX,y.clientY);if(!N)return;let{name:P,path:I,reactComponents:X}=xu(N,Do,Fe),J=Ft(N),ge=y.clientX/window.innerWidth*100,fe=sp(N),he=fe?y.clientY:y.clientY+window.scrollY,Le=N.ownerDocument.defaultView?.getSelection(),Oe;Le&&Le.toString().trim().length>0&&(Oe=Le.toString().trim().slice(0,500));let de=Pl(N),ve=Nl(N);Cs(!1),le({id:Date.now().toString(),x:ge,y:he,clientY:y.clientY,element:P,elementPath:I,selectedText:Oe,boundingBox:{x:J.left,y:fe?J.top:J.top+window.scrollY,width:J.width,height:J.height},nearbyText:os(N),cssClasses:rs(N),isFixed:fe,fullPath:is(N),accessibility:Il(N),computedStyles:ve,computedStylesObj:de,nearbyElements:Rl(N),reactComponents:X??void 0,sourceFile:vu(N),attributes:ts(N,Fe),frame:z0(N,y.clientX,y.clientY),targetElement:N}),U(null)};return ue.addEventListener("click",p,!0),()=>ue.removeEventListener("click",p,!0)},[B,Nn,Qe,R,Wn,pe,Hr,tt.blockInteractions,Do,Fe,kn]),(0,M.useEffect)(()=>{if(!B)return;let p=b=>{let T=(b.key==="Meta"||b.key==="Control")&&!Tr(b),N=b.key==="Shift"&&ks.current;(T||N)&&!Un.current&&kn.length>0&&Rs()},y=()=>{ks.current=!1,Jo([]),U(null),Wr.current=null,Un.current=null,Bu(!1),xi.current?.replaceChildren()};return ue.addEventListener("keyup",p),window.addEventListener("blur",y),()=>{ue.removeEventListener("keyup",p),window.removeEventListener("blur",y)}},[B,kn,Rs]),(0,M.useEffect)(()=>{if(!B||R||Nn||Qe)return;let p=y=>{if(y.button!==0)return;Ms.current=!1;let b=y.composedPath()[0]||y.target;if(Vt(b,"[data-feedback-toolbar]")||Vt(b,"[data-annotation-marker]")||Vt(b,"[data-annotation-popup]"))return;let T=new Set(["P","SPAN","H1","H2","H3","H4","H5","H6","LI","TD","TH","LABEL","BLOCKQUOTE","FIGCAPTION","CAPTION","LEGEND","DT","DD","PRE","CODE","EM","STRONG","B","I","U","S","A","TIME","ADDRESS","CITE","Q","ABBR","DFN","MARK","SMALL","SUB","SUP"]);!Tr(y)&&(T.has(b.tagName)||b.isContentEditable)||(y.preventDefault(),Wr.current={x:y.clientX,y:y.clientY})};return ue.addEventListener("mousedown",p),()=>ue.removeEventListener("mousedown",p)},[B,R,Nn,Qe]),(0,M.useEffect)(()=>{if(!B||R)return;let p=y=>{if(!Wr.current)return;let b=y.clientX-Wr.current.x,T=y.clientY-Wr.current.y,N=b*b+T*T,P=zu*zu;if(!Ao&&N>=P&&(Un.current=Wr.current,Bu(!0),y.preventDefault()),(Ao||N>=P)&&Un.current){if(Es.current){let Ee=Math.min(Un.current.x,y.clientX),be=Math.min(Un.current.y,y.clientY),st=Math.abs(y.clientX-Un.current.x),At=Math.abs(y.clientY-Un.current.y);Es.current.style.transform=`translate(${Ee}px, ${be}px)`,Es.current.style.width=`${st}px`,Es.current.style.height=`${At}px`}let I=Date.now();if(I-Ap.current<b5)return;Ap.current=I;let X=Un.current.x,J=Un.current.y,ge=Math.min(X,y.clientX),fe=Math.min(J,y.clientY),he=Math.max(X,y.clientX),Le=Math.max(J,y.clientY),Oe=(ge+he)/2,de=(fe+Le)/2,ve=new Set,Ce=[[ge,fe],[he,fe],[ge,Le],[he,Le],[Oe,de],[Oe,fe],[Oe,Le],[ge,de],[he,de]];for(let[Ee,be]of Ce){let st=document.elementsFromPoint(Ee,be);for(let At of st)At instanceof HTMLElement&&ve.add(At)}let nt=ue.querySelectorAll("button, a, input, img, p, h1, h2, h3, h4, h5, h6, li, label, td, th, div, span, section, article, aside, nav");for(let Ee of nt)if(Ee instanceof HTMLElement){let be=Ft(Ee),st=be.left+be.width/2,At=be.top+be.height/2,wo=st>=ge&&st<=he&&At>=fe&&At<=Le,oo=Math.min(be.right,he)-Math.max(be.left,ge),Ds=Math.min(be.bottom,Le)-Math.max(be.top,fe),va=oo>0&&Ds>0?oo*Ds:0,tn=be.width*be.height,As=tn>0?va/tn:0;(wo||As>.5)&&ve.add(Ee)}let Et=[],gn=new Set(["BUTTON","A","INPUT","IMG","P","H1","H2","H3","H4","H5","H6","LI","LABEL","TD","TH","SECTION","ARTICLE","ASIDE","NAV"]);for(let Ee of ve){if(Vt(Ee,"[data-feedback-toolbar]")||Vt(Ee,"[data-annotation-marker]"))continue;let be=Ft(Ee);if(!(be.width>window.innerWidth*.8&&be.height>window.innerHeight*.5)&&!(be.width<10||be.height<10)&&be.left<he&&be.right>ge&&be.top<Le&&be.bottom>fe){let st=Ee.tagName,At=gn.has(st);if(!At&&(st==="DIV"||st==="SPAN")){let wo=Ee.textContent&&Ee.textContent.trim().length>0,oo=Ee.onclick!==null||Ee.getAttribute("role")==="button"||Ee.getAttribute("role")==="link"||Ee.classList.contains("clickable")||Ee.hasAttribute("data-clickable");(wo||oo)&&!Ee.querySelector("p, h1, h2, h3, h4, h5, h6, button, a")&&(At=!0)}if(At){let wo=!1;for(let oo of Et)if(oo.left<=be.left&&oo.right>=be.right&&oo.top<=be.top&&oo.bottom>=be.bottom){wo=!0;break}wo||Et.push(be)}}}if(xi.current){let Ee=xi.current;for(;Ee.children.length>Et.length;)Ee.removeChild(Ee.lastChild);Et.forEach((be,st)=>{let At=Ee.children[st];At||(At=document.createElement("div"),At.className=H.selectedElementHighlight,Ee.appendChild(At)),At.style.transform=`translate(${be.left}px, ${be.top}px)`,At.style.width=`${be.width}px`,At.style.height=`${be.height}px`})}}};return ue.addEventListener("mousemove",p,{passive:!0}),()=>ue.removeEventListener("mousemove",p)},[B,R,Ao,zu]),(0,M.useEffect)(()=>{if(!B)return;let p=y=>{let b=Ao,T=Un.current;if(Ao&&T){Ms.current=!0;let N=Math.min(T.x,y.clientX),P=Math.min(T.y,y.clientY),I=Math.max(T.x,y.clientX),X=Math.max(T.y,y.clientY),J=[];ue.querySelectorAll("button, a, input, img, p, h1, h2, h3, h4, h5, h6, li, label, td, th").forEach(de=>{if(!(de instanceof HTMLElement)||Vt(de,"[data-feedback-toolbar]")||Vt(de,"[data-annotation-marker]"))return;let ve=Ft(de);ve.width>window.innerWidth*.8&&ve.height>window.innerHeight*.5||ve.width<10||ve.height<10||ve.left<I&&ve.right>N&&ve.top<X&&ve.bottom>P&&J.push({element:de,rect:ve})});let fe=J.filter(({element:de})=>!J.some(({element:ve})=>ve!==de&&de.contains(ve))),he=y.clientX/window.innerWidth*100,Le=y.clientY+window.scrollY,Oe=(Tr(y)||kn.length>0)&&!R&&!pe;if(fe.length>0)if(Oe){let de=[...kn];for(let{element:ve,rect:Ce}of fe){if(de.some(Ee=>Ee.element===ve))continue;let{name:nt,path:Et,reactComponents:gn}=xu(ve,Do,Fe);de.push({element:ve,rect:Ce,name:nt,path:Et,reactComponents:gn??void 0})}ks.current=y.shiftKey,Tr(y)?Jo(de):Rs(de)}else{let de=fe.reduce((Ee,{rect:be})=>({left:Math.min(Ee.left,be.left),top:Math.min(Ee.top,be.top),right:Math.max(Ee.right,be.right),bottom:Math.max(Ee.bottom,be.bottom)}),{left:1/0,top:1/0,right:-1/0,bottom:-1/0}),ve=fe.slice(0,5).map(({element:Ee})=>kr(Ee).name).join(", "),Ce=fe.length>5?` +${fe.length-5} more`:"",nt=fe[0].element,Et=Pl(nt),gn=Nl(nt);le({id:Date.now().toString(),x:he,y:Le,clientY:y.clientY,element:`${fe.length} elements: ${ve}${Ce}`,elementPath:"multi-select",boundingBox:{x:de.left,y:de.top+window.scrollY,width:de.right-de.left,height:de.bottom-de.top},isMultiSelect:!0,fullPath:is(nt),accessibility:Il(nt),computedStyles:gn,computedStylesObj:Et,nearbyElements:Rl(nt),cssClasses:rs(nt),nearbyText:os(nt),sourceFile:vu(nt),attributes:ts(nt,Fe)})}else if(Oe&&!Tr(y))Rs();else if(!Oe){let de=Math.abs(I-N),ve=Math.abs(X-P);de>20&&ve>20&&le({id:Date.now().toString(),x:he,y:Le,clientY:y.clientY,element:"Area selection",elementPath:`region at (${Math.round(N)}, ${Math.round(P)})`,boundingBox:{x:N,y:P+window.scrollY,width:de,height:ve},isMultiSelect:!0})}U(null)}else b&&(Ms.current=!0);Wr.current=null,Un.current=null,Bu(!1),xi.current&&(xi.current.innerHTML="")};return ue.addEventListener("mouseup",p),()=>ue.removeEventListener("mouseup",p)},[B,Ao,R,pe,Do,Fe,kn,Rs]);let vo=(0,M.useCallback)(async(p,y,b)=>{let T=tt.webhookUrl||A;if(!T||!tt.webhooksEnabled&&!b)return!1;try{return(await fetch(T,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({event:p,timestamp:Date.now(),url:typeof window<"u"?window.location.href:void 0,...y})})).ok}catch(N){return console.warn("[Agentation] Webhook failed:",N),!1}},[A,tt.webhookUrl,tt.webhooksEnabled]),ju=(0,M.useRef)(new Map),[Ps,C5]=(0,M.useState)(null);(0,M.useEffect)(()=>{let p=R;if(!K||!p||p.isSubmitted||p.frame||!p.boundingBox||ju.current.has(p.id))return;let y=(I,X)=>({x:I.x,y:X?I.y:I.y-window.scrollY,width:I.width,height:I.height}),b=y(p.boundingBox,!!p.isFixed),T=p.elementBoundingBoxes?.map(I=>y(I,!1)),N=p.id,P=H1({rect:b,highlights:T}).catch(()=>null);ju.current.set(N,P),P.then(I=>{!I||!vt.current||(C5(X=>X&&X.id!==N?X:{id:N,src:I}),en(X=>X.some(J=>J.id===N&&!J.screenshot)?X.map(J=>J.id===N&&!J.screenshot?{...J,screenshot:I}:J):X))})},[R,K]);let zp=(0,M.useCallback)(async p=>{if(!K)return p;let y=new Promise(b=>Ne(()=>b(null),5e3));return Promise.all(p.map(async b=>{if(b.screenshot)return b;let T=ju.current.get(b.id);if(!T)return b;let N=await Promise.race([T,y]);return N?{...b,screenshot:N}:b}))},[K]),E5=(0,M.useCallback)(p=>{if(!R||R.isSubmitted)return;let y={id:R.id,x:R.x,y:R.y,comment:p,element:R.element,elementPath:R.elementPath,timestamp:Date.now(),selectedText:R.selectedText,boundingBox:R.boundingBox,nearbyText:R.nearbyText,cssClasses:R.cssClasses,isMultiSelect:R.isMultiSelect,isFixed:R.isFixed,fullPath:R.fullPath,accessibility:R.accessibility,computedStyles:R.computedStyles,nearbyElements:R.nearbyElements,reactComponents:R.reactComponents,sourceFile:R.sourceFile,attributes:R.attributes,frame:R.frame,elementBoundingBoxes:R.elementBoundingBoxes,screenshot:Ps?.id===R.id?Ps.src:void 0,...E&&Ut?{sessionId:Ut,url:typeof window<"u"?window.location.href:void 0,status:"pending"}:{}};en(b=>[...b,y]),le({...R,isSubmitted:!0}),no.current=y.id,k?.(y),vo("annotation.add",{annotation:y}),Cs(!0),window.getSelection()?.removeAllRanges(),E&&Ut&&Ct(async()=>{let b=await mo(E,Ut,y);if(o){let T=Go(e);su(e,T.map(N=>N.id===y.id?{...N,id:b.id}:N),Ut)}!vt.current||Zt.current.has(y.id)||b.id!==y.id&&(ra.current.set(b.id,y.id),no.current===y.id&&(no.current=b.id),en(T=>T.map(N=>N.id===y.id?{...N,id:b.id}:N)),zr.current.delete(y.id)&&zr.current.add(b.id))}).catch(b=>{console.warn("[Agentation] Failed to sync annotation:",b)})},[R,Ps,k,vo,E,Ut,Ct,e,o]),ma=(0,M.useCallback)(()=>{Cs(!0)},[]),Yu=(0,M.useCallback)(()=>{le(null),Cs(!1)},[]),Vu=(0,M.useCallback)(p=>{if(Zt.current.has(p))return;Zt.current.add(p);let y=ze.find(b=>b.id===p);pe?.id===p&&(xs(!1),yi(!0)),ia(b=>new Set(b).add(p)),y&&(w?.(y),vo("annotation.delete",{annotation:y})),E&&Ct(()=>zl(E,Ht.current.get(p)??p)).catch(b=>{console.warn("[Agentation] Failed to delete annotation from server:",b)})},[ze,pe,w,vo,E,Ct]),pa=(0,M.useCallback)(p=>{if(!p){ft(null),sn(null),Xt([]);return}if(ft(p.id),p.elementBoundingBoxes?.length){let y=[];for(let b of p.elementBoundingBoxes){let T=b.x+b.width/2,N=b.y+b.height/2-window.scrollY,P=Al(T,N,b);P&&y.push(P)}Xt(y),sn(null)}else if(p.boundingBox){let y=p.boundingBox,b=y.x+y.width/2,T=p.isFixed?y.y+y.height/2:y.y+y.height/2-window.scrollY,N=Al(b,T,y);if(N){let P=Ft(N),I=P.width/y.width,X=P.height/y.height;I<.5||X<.5?sn(null):sn(N)}else sn(null);Xt([])}else sn(null),Xt([])},[]),M5=(0,M.useCallback)(p=>{if(!pe)return;let y={...pe,comment:p};Rr(y),en(b=>b.map(T=>T.id===pe.id?y:T)),C?.(y),vo("annotation.update",{annotation:y}),E&&Ct(()=>cu(E,Ht.current.get(pe.id)??pe.id,{comment:p})).catch(b=>{console.warn("[Agentation] Failed to update annotation on server:",b)}),xs(wu.current||!!di.current?.matches(":hover")),yi(!0)},[pe,C,vo,E,Ct]),T5=(0,M.useCallback)(()=>{xs(wu.current||!!di.current?.matches(":hover")),yi(!0)},[]),L5=(0,M.useCallback)(()=>{bu&&pe&&!R&&ft(pe.id),Rr(null),Pr(null),vs([]),yi(!1)},[bu,pe,R]),fa=(0,M.useCallback)((p,y)=>{if(!p.length&&!y)return;wt(!0);let b={placements:[...fi.current.placements,...p],rearrange:y??fi.current.rearrange};fi.current=b,vp(b.placements),wp(b.rearrange),clearTimeout(Ae.current),Ae.current=Ne(()=>{yo(T=>T.filter(N=>!b.placements.includes(N))),eo(T=>T===b.rearrange?null:T),fi.current={placements:[],rearrange:null},vp([]),wp(null),Ae.current=void 0,Ve()},200)},[Ve]),er=(0,M.useCallback)(()=>{if(!vt.current)return;let p=new Map(aa.current.map(X=>[X.id,X])),y=[];for(let X of ze){let J=p.get(Ht.current.get(X.id)??X.id)??p.get(X.id);J&&J.comment===X.comment&&!Zt.current.has(J.id)&&!y.includes(J)&&y.push(J)}let b=y.length,T=bp.current,N=Pe.filter(X=>T.designPlacements.includes(X)&&!fi.current.placements.includes(X)),P=_e===T.rearrangeState&&_e!==fi.current.rearrange?_e:null,I=In.filter(X=>Iu.current.includes(X));if(!(b===0&&I.length===0&&N.length===0&&!P)){for(let X of y)Zt.current.add(X.id),ht.current.add(X.id),at.current.add(X.id);if(ia(X=>new Set([...X,...y.map(J=>J.id)])),S?.(y),vo("annotations.clear",{annotations:y}),E&&Promise.all(y.map(X=>Ct(()=>zl(E,Ht.current.get(X.id)??X.id)).catch(J=>{console.warn("[Agentation] Failed to delete annotation from server:",J)}))),wt(!0),y5(X=>X.filter(J=>!I.includes(J))),I.length>0&&I.length===Iu.current.length){let X=Du.current;X?.getContext("2d")?.clearRect(0,0,X.width,X.height)}fa(N,P),yt===T.blankCanvas&&an===T.wireframePurpose&&Pe===T.designPlacements&&_e===T.rearrangeState&&(yt&&fp(!1),an&&Zl(""),$r.current={rearrange:null,placements:[]},Bl(e)),Ve()}},[e,ze,In,Pe,_e,yt,an,S,vo,E,Ct,Ve,fa]),ga=(0,M.useCallback)(async()=>{let p=Je.start(),y=typeof window<"u"?window.location.pathname+window.location.search+window.location.hash:e,b=Qe&&yt,T;if(b){if(Pe.length===0&&!_e&&!an)return;T=r?jl(y,r):""}else{if(T=Xm(ze,y,tt.outputDetail,{appName:r}),!T&&In.length===0&&Pe.length===0&&!_e)return;T||(T=jl(y,r))}if(!b&&In.length>0){let P=new Set;for(let ge of ze)ge.drawingIndex!=null&&P.add(ge.drawingIndex);let I=Du.current;I&&(I.style.visibility="hidden");let X=[],J=window.scrollY;for(let ge=0;ge<In.length;ge++){if(P.has(ge))continue;let fe=In[ge];if(fe.points.length<2)continue;let he=fe.fixed?fe.points:fe.points.map(Bt=>({x:Bt.x,y:Bt.y-J})),Le=1/0,Oe=1/0,de=-1/0,ve=-1/0;for(let Bt of he)Le=Math.min(Le,Bt.x),Oe=Math.min(Oe,Bt.y),de=Math.max(de,Bt.x),ve=Math.max(ve,Bt.y);let Ce=de-Le,nt=ve-Oe,Et=Math.hypot(Ce,nt),gn=he[0],Ee=he[he.length-1],be=Math.hypot(Ee.x-gn.x,Ee.y-gn.y),st,At=be<Et*.35,wo=Ce/Math.max(nt,1);if(At&&Et>20){let Bt=Math.max(Ce,nt)*.15,tr=0;for(let Ur of he){let I5=Ur.x-Le<Bt,D5=de-Ur.x<Bt,A5=Ur.y-Oe<Bt,$5=ve-Ur.y<Bt;(I5||D5)&&(A5||$5)&&tr++}st=tr>he.length*.15?"box":"circle"}else wo>3&&nt<40?st="underline":be>Et*.5?st="arrow":st="drawing";let oo=Math.min(10,he.length),Ds=Math.max(1,Math.floor(he.length/oo)),va=new Set,tn=[],As=[gn];for(let Bt=Ds;Bt<he.length-1;Bt+=Ds)As.push(he[Bt]);As.push(Ee);for(let Bt of As){let tr=Dl(Bt.x,Bt.y);if(!tr||va.has(tr)||Vt(tr,"[data-feedback-toolbar]"))continue;va.add(tr);let{name:Ur}=kr(tr);tn.includes(Ur)||tn.push(Ur)}let wa=`${Math.round(Le)},${Math.round(Oe)} \u2192 ${Math.round(de)},${Math.round(ve)}`,bi;(st==="circle"||st==="box")&&tn.length>0?bi=`${st==="box"?"Boxed":"Circled"} **${tn[0]}**${tn.length>1?` (and ${tn.slice(1).join(", ")})`:""} (region: ${wa})`:st==="underline"&&tn.length>0?bi=`Underlined **${tn[0]}** (${wa})`:st==="arrow"&&tn.length>=2?bi=`Arrow from **${tn[0]}** to **${tn[tn.length-1]}** (${Math.round(gn.x)},${Math.round(gn.y)} \u2192 ${Math.round(Ee.x)},${Math.round(Ee.y)})`:tn.length>0?bi=`${st==="arrow"?"Arrow":"Drawing"} near **${tn.join("**, **")}** (region: ${wa})`:bi=`Drawing at ${wa}`,X.push(bi)}I&&(I.style.visibility=""),X.length>0&&(T+=`
**Drawings:**
`,X.forEach((ge,fe)=>{T+=`${fe+1}. ${ge}
`}))}if((Pe.length>0||b&&an)&&(T+=`
`+Dm(Pe,{width:window.innerWidth,height:window.innerHeight},{blankCanvas:yt,wireframePurpose:an||void 0},tt.outputDetail)),_e){let P=Am(_e,tt.outputDetail,{width:window.innerWidth,height:window.innerHeight});P&&(T+=`
`+P)}if(T=Zm(ze,T,l),!T){He(!1);return}let N=!h||await W1(T);g?.(T),Je.isCurrent(p)&&(He(N),N&&(Je.schedule(p,()=>He(!1),2e3),tt.autoClearAfterCopy&&Je.schedule(p,er,500)))},[ze,In,Pe,_e,yt,Qe,yp,an,e,tt.outputDetail,Do,Fe,tt.autoClearAfterCopy,er,Je,h,l,r,g]),Xu=i5(tt.webhookUrl)||i5(A||""),jn=x!=null||Xu&&!tt.webhooksEnabled,ha=B?jn?337:297:44,ya=(0,M.useCallback)(async()=>{let p=it.start(),y=typeof window<"u"?window.location.href:e,b=typeof window<"u"?window.location.pathname+window.location.search+window.location.hash:e;if(ze.length===0&&Pe.length===0&&!_e)return;Ze("sending");let T=await zp(ze);if(!it.isCurrent(p))return;let N=Xm(T,b,tt.outputDetail,{appName:r,screenshotReference:q});if(N||(N=jl(b,r)),Pe.length>0&&(N+=`
`+Dm(Pe,{width:window.innerWidth,height:window.innerHeight},{blankCanvas:yt,wireframePurpose:an||void 0},tt.outputDetail)),_e){let J=Am(_e,tt.outputDetail,{width:window.innerWidth,height:window.innerHeight});J&&(N+=`
`+J)}let P=!0;try{await x?.(N,T)}catch(J){console.warn("[Agentation] Submit callback failed:",J),P=!1}if(!it.isCurrent(p))return;let I=Xu?await vo("submit",{output:N,annotations:T,url:y},!0):!0,X=P&&I&&jn;it.isCurrent(p)&&(Ze(X?"sent":"failed"),it.schedule(p,()=>Ze("idle"),2500),X&&tt.autoClearAfterCopy&&it.schedule(p,er,500))},[x,r,vo,ze,Pe,_e,yt,yp,e,tt.outputDetail,Do,Fe,tt.autoClearAfterCopy,er,Xu,jn,it,zp,q]);(0,M.useEffect)(()=>{let y=(P=!1)=>{Ss.current?.dragging&&(oa.current=P,Ip(!1)),Ss.current=null},b=P=>{let I=Ss.current;if(!I)return;if((P.buttons&1)===0){y();return}let X=P.clientX-I.x,J=P.clientY-I.y,ge=Math.sqrt(X*X+J*J);if(!I.dragging&&ge>10&&(I.dragging=!0,Ip(!0)),I.dragging){let fe=I.toolbarX+X,he=I.toolbarY+J,Le=20,Oe=337,de=44,Ce=Oe-ha,nt=Le-Ce,Et=window.innerWidth-Le-Oe;fe=Math.max(nt,Math.min(Et,fe)),he=Math.max(Le,Math.min(window.innerHeight-de-Le,he)),Ou({x:fe,y:he})}},T=()=>y(!0),N=()=>y();return ue.addEventListener("mousemove",b),ue.addEventListener("mouseup",T,!0),window.addEventListener("blur",N),()=>{ue.removeEventListener("mousemove",b),ue.removeEventListener("mouseup",T,!0),window.removeEventListener("blur",N)}},[ha]);let R5=(0,M.useCallback)(p=>{if(oa.current=!1,Ss.current=null,p.button!==0||p.target.closest("button")&&(p.target.closest("button")!==xe.current||B)||p.target.closest("[data-agentation-settings-panel]"))return;let y=p.currentTarget.parentElement;if(!y)return;let b=Ft(y);Ss.current={x:p.clientX,y:p.clientY,toolbarX:b.left,toolbarY:b.top,dragging:!1}},[B]);(0,M.useLayoutEffect)(()=>{if(!Dt)return;let p=()=>{let N=Dt.x,P=Dt.y,J=20-(337-ha),ge=window.innerWidth-20-337;N=Math.max(J,Math.min(ge,N)),P=Math.max(20,Math.min(window.innerHeight-44-20,P)),(N!==Dt.x||P!==Dt.y)&&Ou({x:N,y:P})};return p(),window.addEventListener("resize",p),()=>window.removeEventListener("resize",p)},[Dt,ha]),(0,M.useEffect)(()=>{if(!i)return;let p=b=>{if(b.defaultPrevented||b.isComposing||b.altKey)return;let T=b.composedPath()[0]||b.target,N=T.tagName==="INPUT"||T.tagName==="TEXTAREA"||T.tagName==="SELECT"||T.isContentEditable;if(b.key==="Escape"){if(c&&!R&&!pe&&(B||ln||Qe||Nn||kn.length)&&(b.preventDefault(),b.stopPropagation()),ln){b.preventDefault(),Ir(!1),Se.current?.focus();return}if(Qe){_i?Ar(null):vi();return}if(Nn){ta(!1);return}if(kn.length>0){Jo([]);return}R||pe||B&&Z!=="hidden"&&(cn(),Ls())}if(Z!=="hidden"&&(b.metaKey||b.ctrlKey)&&b.shiftKey&&(b.key==="f"||b.key==="F")){b.preventDefault(),cn(),B?Ls():(xe.current?.blur(),gt.current=!0,dt(!0));return}!B||N||b.metaKey||b.ctrlKey||b.repeat||((b.key==="p"||b.key==="P")&&(b.preventDefault(),cn(),da()),(b.key==="l"||b.key==="L")&&(b.preventDefault(),cn(),Nn&&ta(!1),ln&&Ir(!1),R&&ma(),Qe?vi():ca()),(b.key==="h"||b.key==="H")&&ze.length>0&&(b.preventDefault(),cn(),Zn(P=>!P)),(b.key==="c"||b.key==="C")&&(ze.length>0||Pe.length>0||_e)&&(b.preventDefault(),cn(),ga()),(b.key==="x"||b.key==="X")&&(ze.length>0||Pe.length>0||_e)&&(b.preventDefault(),cn(),er(),Pe.length>0&&yo([]),_e&&eo(null)),(b.key==="s"||b.key==="S")&&ze.length>0&&jn&&We==="idle"&&(b.preventDefault(),cn(),ya()))},y=!!c;return ue.addEventListener("keydown",p,y),()=>ue.removeEventListener("keydown",p,y)},[i,Z,c,pe,B,Nn,Qe,_i,Pe,_e,R,ze.length,jn,We,ya,da,ga,er,kn,ln,Ls,ca,vi]);let $o=ze.length>0,xa=H0(),Ns=ze.filter(p=>p.kind!=="placement"&&p.kind!=="rearrange"),Hp=Ns.flatMap((p,y)=>{let b=xa(p);return b?[{annotation:b,index:y}]:[]}),Wp=R&&!R.isSubmitted?xa({...R,comment:"",timestamp:0}):null,Up=[...Ye?Hp.map(p=>({...p,pending:!1})):[],...Wp?[{annotation:Wp,index:Ns.length,pending:!0}]:[]];(0,M.useEffect)(()=>{let p=new Set(Ye&&!Lr?Hp.map(({annotation:y})=>y.id):[]);no.current&&!p.has(no.current)&&(no.current=null);for(let y of Zo)p.has(y)||Wu(y)}),(0,M.useEffect)(()=>{pe&&Zo.has(pe.id)&&(xs(!1),yi(!0))},[pe,Zo]);let jp=(0,M.useCallback)(p=>{!L&&p.id!==no.current&&pa(p)},[L,pa]),Yp=(0,M.useCallback)(p=>{et===p&&pa(null)},[et,pa]),Vp=(0,M.useCallback)((p,y)=>{if(pe&&!Hr){sa.current?.shake();return}Wn&&Yu(),tt.markerClickBehavior==="delete"?Vu(p.id):_a(p,y)},[tt.markerClickBehavior,Vu,_a,Wn,Yu,pe,Hr]),Dn=pe??(la&&!R&&!Te?ze.find(p=>p.id===et&&!Zo.has(p.id)):null),Xp=Pn?"Resume animations":"Pause animations",Qp=Qe?"Exit layout mode":"Layout mode",Gp=go?"Hide markers":"Show markers",Qu=l!=="markdown"&&!Zm(ze,"",l),qp=typeof l=="object"?`Copy ${l.attribute}`:l==="source"?"Copy source paths":l==="classes"?"Copy classes":Qe&&yt?"Copy layout":"Copy feedback",wi=B?0:-1,Kp=!Qu&&(Qe&&yt?Pe.length>0||!!_e?.sections?.length:$o||In.length>0||Pe.length>0||!!_e?.sections?.length),P5=$o||In.length>0||Pe.length>0||!!_e?.sections?.length,Gu={active:B,frozen:Pn,layout:Qe,marksVisible:go,notes:Ns.length,canCopy:Kp,canSend:$o&&jn&&We!=="sending",canClear:P5,sendState:We,settings:tt},qu=(0,M.useRef)(Gu);qu.current=Gu;let Fo=(0,M.useRef)(null);Fo.current={setActive:p=>{p!==B&&(p?dt(!0):Ls())},toggleFreeze:da,toggleLayout:()=>{Nn&&ta(!1),R&&ma(),Qe?vi():ca()},setMarksVisible:p=>Zn(p),copy:()=>Kp?ga():Promise.resolve(),send:()=>$o&&jn&&We==="idle"?ya():Promise.resolve(),clearAll:()=>{er(),Pe.length>0&&yo([]),_e&&eo(null)},updateSettings:Rp};let N5=JSON.stringify(Gu);(0,M.useEffect)(()=>{Xe&&me?.(qu.current)},[N5,Xe]),(0,M.useEffect)(()=>{!Xe||!ye||ye({setActive:p=>Fo.current.setActive(p),toggleFreeze:()=>Fo.current.toggleFreeze(),toggleLayout:()=>Fo.current.toggleLayout(),setMarksVisible:p=>Fo.current.setMarksVisible(p),copy:()=>Fo.current.copy(),send:()=>Fo.current.send(),clearAll:()=>Fo.current.clearAll(),updateSettings:p=>Fo.current.updateSettings(p),getState:()=>qu.current})},[Xe]);let Jp=(0,M.useRef)(!1);(0,M.useEffect)(()=>{if(Xe){if(!Jp.current){Jp.current=!0;return}Me?.(ze)}},[ze]);let Is=Z==="hidden",Zp=(0,V.jsx)(Ny,{placement:Is?"topRight":"above",visible:Qe&&B,activeType:_i,onSelect:p=>{Ar(_i===p?null:p)},isDarkMode:xo,sectionCount:_e?.sections.length??0,onDetectSections:()=>{let p=Fy(),y=_e?.sections??[],b=new Set(y.map(I=>I.selector)),T=p.filter(I=>!b.has(I.selector)),N=[...y,...T],P=[..._e?.originalOrder??[],...T.map(I=>I.id)];eo({sections:N,originalOrder:P,detectedAt:Date.now()})},placementCount:Pe.length,onClearPlacements:()=>{fa(Pe,_e)},blankCanvas:yt,onBlankCanvasChange:p=>{let y={sections:[],originalOrder:[],detectedAt:Date.now()};p?(Lu.current={rearrange:_e,placements:Pe},eo($r.current.rearrange||y),yo($r.current.placements),Ar(null)):($r.current={rearrange:_e,placements:Pe},eo(Lu.current.rearrange||y),yo(Lu.current.placements)),fp(p)},wireframePurpose:an,onWireframePurposeChange:Zl,Tooltip:Lo,onDragStart:(p,y)=>{y.preventDefault();let b=ee[p],T=null,N=!1,P=y.clientX,I=y.clientY,J=y.target.closest("[data-feedback-toolbar]")?.getBoundingClientRect().top??window.innerHeight,ge=he=>{let Le=he.clientX-P,Oe=he.clientY-I;if(!N&&(Math.abs(Le)>4||Math.abs(Oe)>4)&&(N=!0,T=document.createElement("div"),T.className=`${$.dragPreview}${yt?` ${$.dragPreviewWireframe}`:""}`,ie.current?.appendChild(T)),!T)return;let de=Is?Math.hypot(Le,Oe):Math.max(0,J-he.clientY),ve=Math.min(1,de/180),Ce=1-Math.pow(1-ve,2),nt=28,Et=20,gn=Math.min(140,b.width*.18),Ee=Math.min(90,b.height*.18),be=nt+(gn-nt)*Ce,st=Et+(Ee-Et)*Ce;T.style.width=`${be}px`,T.style.height=`${st}px`,T.style.left=`${he.clientX-be/2}px`,T.style.top=`${he.clientY-st/2}px`,T.style.opacity=`${.5+.5*Ce}`,T.textContent=Ce>.25?p:""},fe=he=>{if(window.removeEventListener("mousemove",ge),window.removeEventListener("mouseup",fe),T&&T.remove(),N){let Le=b.width,Oe=b.height,de=window.scrollY,ve=Math.max(0,he.clientX-Le/2),Ce=Math.max(0,he.clientY+de-Oe/2),nt={id:`dp-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,type:p,x:ve,y:Ce,width:Le,height:Oe,scrollY:de,timestamp:Date.now()};yo(Et=>[...Et,nt]),Ar(null),bs.current=new Set,xp(Et=>Et+1)}};window.addEventListener("mousemove",ge),window.addEventListener("mouseup",fe)}});return!Xe||Lr?null:(0,V.jsxs)(ry,{host:"agentation-toolbar",className:Y,children:[(0,V.jsxs)("style",{"data-agentation-styles":"toolbar",children:[Dk,$k]}),(0,V.jsxs)("div",{ref:ie,className:H.positionContext,style:{display:"contents"},"data-agentation-theme":xo?"dark":"light","data-agentation-accent":tt.annotationColorId,"data-agentation-root":"",children:[Is&&(0,V.jsx)("div",{className:H.hostPaletteAnchor,"data-feedback-toolbar":!0,children:Zp}),!Is&&(0,V.jsx)("div",{className:H.toolbar,"data-feedback-toolbar":!0,"data-agentation-toolbar":!0,"data-dragging":na||void 0,style:Dt?{left:Dt.x,top:Dt.y,right:"auto",bottom:"auto"}:void 0,children:(0,V.jsxs)("div",{className:`${H.toolbarContainer} ${B?H.expanded:H.collapsed} ${Tp?H.entrance:""} ${Ko?H.hiding:""} ${jn?H.serverConnected:""}`,onMouseDown:R5,children:[(0,V.jsxs)("div",{className:`${H.controlsContent} ${B?H.visible:H.hidden} ${Dt&&Dt.y<100?H.tooltipBelow:""} ${pp||ln?H.tooltipsHidden:""} ${Au?H.tooltipsInSession:""}`,ref:p=>{De.current=p,p?.toggleAttribute("inert",!B)},role:"group","aria-label":"Feedback controls","aria-hidden":!B,onMouseEnter:Ep,onMouseLeave:Mp,children:[(0,V.jsxs)("div",{className:`${H.buttonWrapper} ${Dt&&Dt.x<120?H.buttonWrapperAlignLeft:""}`,children:[(0,V.jsx)("button",{className:H.controlButton,onClick:p=>{p.stopPropagation(),cn(),da()},"data-active":Pn,"aria-label":Xp,"aria-pressed":Pn,tabIndex:wi,children:(0,V.jsx)(my,{size:24,isPaused:Pn})}),(0,V.jsxs)("span",{className:H.buttonTooltip,children:[Xp,i&&(0,V.jsx)("span",{className:H.shortcut,children:"P"})]})]}),(0,V.jsxs)("div",{className:H.buttonWrapper,children:[(0,V.jsx)("button",{className:`${H.controlButton} ${xo?"":H.light}`,onClick:p=>{p.stopPropagation(),cn(),Nn&&ta(!1),ln&&Ir(!1),R&&ma(),Qe?vi():ca()},"data-active":Qe,"aria-label":Qp,"aria-pressed":Qe,tabIndex:wi,style:Qe&&yt?{color:"#f97316",background:"rgba(249, 115, 22, 0.25)"}:void 0,children:(0,V.jsx)(wy,{size:21})}),(0,V.jsxs)("span",{className:H.buttonTooltip,children:[Qp,i&&(0,V.jsx)("span",{className:H.shortcut,children:"L"})]})]}),(0,V.jsxs)("div",{className:H.buttonWrapper,children:[(0,V.jsx)("button",{className:H.controlButton,onClick:p=>{p.stopPropagation(),cn(),Zn(!go)},disabled:!$o||Qe,"aria-label":Gp,tabIndex:wi,children:(0,V.jsx)(_y,{size:24,isOpen:go})}),(0,V.jsxs)("span",{className:H.buttonTooltip,children:[Gp,i&&(0,V.jsx)("span",{className:H.shortcut,children:"H"})]})]}),(0,V.jsxs)("div",{className:H.buttonWrapper,children:[(0,V.jsx)("button",{className:`${H.controlButton} ${we?H.statusShowing:""}`,onClick:p=>{p.stopPropagation(),cn(),ga()},disabled:Qu||(Qe&&yt?Pe.length===0&&!_e?.sections?.length:!$o&&In.length===0&&Pe.length===0&&!_e?.sections?.length),"data-active":we,"aria-label":qp,tabIndex:wi,children:(0,V.jsx)(uy,{size:24,copied:we,tint:Qe&&yt&&(Pe.length>0||_e?.sections?.length)?"#f97316":void 0})}),(0,V.jsxs)("span",{className:H.buttonTooltip,children:[Qu?"No matching metadata":qp,i&&(0,V.jsx)("span",{className:H.shortcut,children:"C"})]})]}),(0,V.jsxs)("div",{className:`${H.buttonWrapper} ${H.sendButtonWrapper} ${B&&jn?H.sendButtonVisible:""}`,children:[(0,V.jsxs)("button",{className:`${H.controlButton} ${We==="sent"||We==="failed"?H.statusShowing:""}`,onClick:p=>{p.stopPropagation(),cn(),ya()},disabled:!$o||!jn||We==="sending","data-no-hover":We==="sent"||We==="failed",tabIndex:B&&jn?0:-1,"aria-label":"Send Annotations","aria-hidden":!jn,children:[(0,V.jsx)(dy,{size:24,state:We}),$o&&We==="idle"&&(0,V.jsx)("span",{className:H.buttonBadge,children:ze.length})]}),(0,V.jsxs)("span",{className:H.buttonTooltip,children:["Send Annotations",i&&(0,V.jsx)("span",{className:H.shortcut,children:"S"})]})]}),(0,V.jsxs)("div",{className:H.buttonWrapper,children:[(0,V.jsx)("button",{className:H.controlButton,onClick:p=>{p.stopPropagation(),cn(),er()},disabled:!$o&&In.length===0&&Pe.length===0&&!_e?.sections?.length,"data-danger":!0,"aria-label":"Clear all",tabIndex:wi,children:(0,V.jsx)(fy,{size:24})}),(0,V.jsxs)("span",{className:H.buttonTooltip,children:["Clear all",i&&(0,V.jsx)("span",{className:H.shortcut,children:"X"})]})]}),(0,V.jsxs)("div",{className:H.buttonWrapper,children:[(0,V.jsx)("button",{ref:Se,"aria-label":"Settings","aria-expanded":ln,tabIndex:wi,className:H.controlButton,onClick:p=>{p.stopPropagation(),cn(),Qe&&vi(),Ge.current=!ln&&p.detail===0,Ir(!ln)},children:(0,V.jsx)(py,{size:24})}),E&&Or!=="disconnected"&&(0,V.jsx)("span",{className:`${H.mcpIndicator} ${H[Or]} ${ln?H.hidden:""}`,title:Or==="connected"?"MCP Connected":"MCP Connecting..."}),(0,V.jsx)("span",{className:H.buttonTooltip,children:"Settings"})]}),(0,V.jsx)("div",{className:H.divider}),(0,V.jsx)("div",{className:H.togglePlaceholder,"aria-hidden":"true"})]}),(0,V.jsxs)("div",{className:`${H.buttonWrapper} ${H.toggleWrapper} ${Dt&&Dt.y<100?H.tooltipBelow:""} ${!B||pp||ln?H.tooltipsHidden:""} ${Au?H.tooltipsInSession:""} ${Dt&&typeof window<"u"&&Dt.x>window.innerWidth-120?H.buttonWrapperAlignRight:""}`,onMouseEnter:Ep,onMouseLeave:Mp,children:[(0,V.jsx)("button",{ref:xe,type:"button",className:`${H.toggleContent} ${B?H.expandedToggle:""}`,"aria-label":B?"Exit":"Start feedback mode","aria-expanded":B,"aria-keyshortcuts":i?"Meta+Shift+F Control+Shift+F":void 0,title:B?void 0:i?"Start feedback mode (\u2318\u21E7F / Ctrl+Shift+F)":"Start feedback mode",onClick:p=>{if(oa.current){oa.current=!1,p.preventDefault();return}p.stopPropagation(),B?(cn(),Ls()):(p.currentTarget.blur(),gt.current=p.detail===0,dt(!0))},children:(0,V.jsxs)("span",{className:H.toggleIcon,children:[(0,V.jsx)(Ey,{active:B}),Ns.length>0&&(0,V.jsx)("span",{className:`${H.badge} ${B?H.fadeOut:""} ${Tp?H.entrance:""}`,children:Ns.length})]})}),(0,V.jsxs)("span",{className:H.buttonTooltip,"aria-hidden":!B,children:["Exit",i&&(0,V.jsx)("span",{className:H.shortcut,children:"Esc"})]})]}),!Is&&Zp,(0,V.jsx)(t5,{settings:tt,onSettingsChange:Rp,isDarkMode:xo,onToggleTheme:w5,isDevMode:Pp,connectionStatus:Or,endpoint:E,onExited:k5,isOpen:B&&ln,toolbarNearBottom:!!Dt&&Dt.y<230,settingsPage:d5,onSettingsPageChange:mp,onHideToolbar:S5})]})}),(Qe||Dr)&&(0,V.jsx)("div",{className:`${$.blankCanvas} ${gp?$.visible:""} ${m5?$.gridActive:""}`,style:{"--canvas-opacity":hp},"data-feedback-toolbar":!0}),Qe&&yt&&gp&&(0,V.jsxs)("div",{className:$.wireframeNotice,"data-feedback-toolbar":!0,children:[(0,V.jsxs)("div",{className:$.wireframeOpacityRow,children:[(0,V.jsx)("span",{className:$.wireframeOpacityLabel,children:"Toggle Opacity"}),(0,V.jsx)("input",{type:"range",className:$.wireframeOpacitySlider,min:0,max:1,step:.01,value:hp,onChange:p=>_5(Number(p.target.value))})]}),(0,V.jsxs)("div",{className:$.wireframeNoticeTitleRow,children:[(0,V.jsx)("span",{className:$.wireframeNoticeTitle,children:"Wireframe Mode"}),(0,V.jsx)("span",{className:$.wireframeNoticeDivider}),(0,V.jsx)("button",{className:$.wireframeStartOver,onClick:()=>{fa(Pe,_e),$r.current={rearrange:null,placements:[]},Zl(""),Bl(e)},children:"Start Over"})]}),"Drag components onto the canvas.",(0,V.jsx)("br",{}),"Copied output will only include the wireframed layout."]}),(Qe||Dr)&&(0,V.jsx)(Py,{placements:Pe,onChange:yo,activeComponent:Dr?null:_i,onActiveComponentChange:Ar,isDarkMode:xo,exiting:Dr,onInteractionChange:p5,passthrough:!_i,extraSnapRects:_e?.sections.map(p=>p.currentRect),deselectSignal:f5,clearingPlacements:mi,wireframe:yt,onSelectionChange:(p,y)=>{bs.current=p,y||(ea.current=new Set,h5(b=>b+1))},onDragMove:(p,y)=>{let b=ea.current;if(!(!b.size||!_e)){if(!to.current){to.current=new Map;for(let T of _e.sections)b.has(T.id)&&to.current.set(T.id,{x:T.currentRect.x,y:T.currentRect.y})}for(let T of _e.sections){if(!b.has(T.id)||!to.current.get(T.id))continue;let P=ie.current?.querySelector(`[data-rearrange-section="${T.id}"]`);P&&(P.style.transform=`translate(${p}px, ${y}px)`)}}},onDragEnd:(p,y,b)=>{let T=ea.current,N=to.current;if(to.current=null,!(!T.size||!_e||!N)){for(let P of T){let I=ie.current?.querySelector(`[data-rearrange-section="${P}"]`);I&&(I.style.transform="")}b&&eo(P=>P&&{...P,sections:P.sections.map(I=>{let X=N.get(I.id);return X?{...I,currentRect:{...I.currentRect,x:Math.max(0,X.x+p),y:Math.max(0,X.y+y)}}:I})})}}}),(Qe||Dr)&&_e&&(0,V.jsx)(jy,{rearrangeState:_e,onChange:eo,isDarkMode:xo,exiting:Dr,blankCanvas:yt,extraSnapRects:Pe.map(p=>({x:p.x,y:p.y,width:p.width,height:p.height})),clearing:_e===pi,deselectSignal:g5,onSelectionChange:(p,y)=>{ea.current=p,y||(bs.current=new Set,xp(b=>b+1))},onDragMove:(p,y)=>{let b=bs.current;if(b.size){if(!to.current){to.current=new Map;for(let T of Pe)b.has(T.id)&&to.current.set(T.id,{x:T.x,y:T.y})}for(let T of b){let N=ie.current?.querySelector(`[data-design-placement="${T}"]`);N&&(N.style.transform=`translate(${p}px, ${y}px)`)}}},onDragEnd:(p,y,b)=>{let T=bs.current,N=to.current;if(to.current=null,!(!T.size||!N)){for(let P of T){let I=ie.current?.querySelector(`[data-design-placement="${P}"]`);I&&(I.style.transform="")}b&&yo(P=>P.map(I=>{let X=N.get(I.id);return X?{...I,x:Math.max(0,X.x+p),y:Math.max(0,X.y+y)}:I}))}}}),(0,V.jsx)("canvas",{ref:Du,className:`${H.drawCanvas} ${Nn?H.active:""}`,"aria-hidden":"true",style:{opacity:la?1:0,transition:"opacity 0.15s ease"},"data-feedback-toolbar":!0}),(0,V.jsx)("div",{className:H.markersLayer,"data-feedback-toolbar":!0,children:Up.filter(({annotation:p})=>!p.isFixed).map(({annotation:p,index:y,pending:b},T,N)=>(0,V.jsx)(np,{annotation:p,pending:b,globalIndex:y,layerIndex:T,layerSize:N.length,isExiting:b?Wn:L,isClearing:ht.current.has(p.id),isAnimated:zr.current.has(p.id),isNew:no.current===p.id,onEnterComplete:Dp,isHovered:!L&&et===p.id,isRemoving:Zo.has(p.id),onRemoveComplete:Wu,isEditingAny:!!pe,renumberFrom:Io,markerClickBehavior:tt.markerClickBehavior,onHoverEnter:jp,onHoverLeave:Yp,onClick:Vp,onContextMenu:_a},ra.current.get(p.id)??p.id))}),(0,V.jsx)("div",{className:H.fixedMarkersLayer,"data-feedback-toolbar":!0,children:Up.filter(({annotation:p})=>p.isFixed).map(({annotation:p,index:y,pending:b},T,N)=>(0,V.jsx)(np,{annotation:p,pending:b,globalIndex:y,layerIndex:T,layerSize:N.length,isExiting:b?Wn:L,isClearing:ht.current.has(p.id),isAnimated:zr.current.has(p.id),isNew:no.current===p.id,onEnterComplete:Dp,isHovered:!L&&et===p.id,isRemoving:Zo.has(p.id),onRemoveComplete:Wu,isEditingAny:!!pe,renumberFrom:Io,markerClickBehavior:tt.markerClickBehavior,onHoverEnter:jp,onHoverLeave:Yp,onClick:Vp,onContextMenu:_a},ra.current.get(p.id)??p.id))}),B&&z&&!R&&!pe&&!up&&!Ao&&(0,V.jsx)(n5,{x:se.x,y:se.y,elementName:z.elementName,reactComponents:z.reactComponents}),B&&(0,V.jsxs)("div",{className:H.overlay,"data-feedback-toolbar":!0,style:R||pe?{zIndex:"inherit"}:void 0,children:[z?.rect&&!R&&!up&&!Ao&&(0,V.jsx)("div",{className:`${H.hoverHighlight} ${H.enter}`,style:{left:z.rect.left,top:z.rect.top,width:z.rect.width,height:z.rect.height,borderColor:"color-mix(in srgb, var(--agentation-color-accent) 50%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 4%, transparent)",...z.isPiercing?{borderStyle:"dashed"}:{}}}),kn.filter(p=>p.element.isConnected).map((p,y)=>{let b=Ft(p.element),T=kn.length>1;return(0,V.jsx)("div",{className:T?H.multiSelectOutline:H.singleSelectOutline,style:{position:"fixed",left:b.left,top:b.top,width:b.width,height:b.height,...T?{}:{borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}}},y)}),et&&!R&&(()=>{let p=ze.find(N=>N.id===et);if(!p?.boundingBox)return null;if(p.elementBoundingBoxes?.length)return ho.length>0?ho.filter(N=>N.isConnected).map((N,P)=>{let I=Ft(N);return(0,V.jsx)("div",{className:`${H.multiSelectOutline} ${H.enter}`,style:{left:I.left,top:I.top,width:I.width,height:I.height}},`hover-outline-live-${P}`)}):p.elementBoundingBoxes.map((N,P)=>(0,V.jsx)("div",{className:`${H.multiSelectOutline} ${H.enter}`,style:{left:N.x,top:N.y-Nr,width:N.width,height:N.height}},`hover-outline-${P}`));let y=Wt&&Wt.isConnected?Ft(Wt):null,b=y?{x:y.left,y:y.top,width:y.width,height:y.height}:{x:p.boundingBox.x,y:p.isFixed?p.boundingBox.y:p.boundingBox.y-Nr,width:p.boundingBox.width,height:p.boundingBox.height},T=p.isMultiSelect;return(0,V.jsx)("div",{className:`${T?H.multiSelectOutline:H.singleSelectOutline} ${H.enter}`,style:{left:b.x,top:b.y,width:b.width,height:b.height,...T?{}:{borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}}})})(),R&&(0,V.jsxs)(V.Fragment,{children:[R.multiSelectElements?.length?R.multiSelectElements.filter(p=>p.isConnected).map((p,y)=>{let b=Ft(p);return(0,V.jsx)("div",{className:`${H.multiSelectOutline} ${Wn?H.exit:H.enter}`,style:{left:b.left,top:b.top,width:b.width,height:b.height}},`pending-multi-${y}`)}):R.targetElement&&R.targetElement.isConnected?(()=>{let p=Ft(R.targetElement);return(0,V.jsx)("div",{className:`${H.singleSelectOutline} ${Wn?H.exit:H.enter}`,style:{left:p.left,top:p.top,width:p.width,height:p.height,borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}})})():R.boundingBox&&(0,V.jsx)("div",{className:`${R.isMultiSelect?H.multiSelectOutline:H.singleSelectOutline} ${Wn?H.exit:H.enter}`,style:{left:R.boundingBox.x,top:R.boundingBox.y-Nr,width:R.boundingBox.width,height:R.boundingBox.height,...R.isMultiSelect?{}:{borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}}}),(()=>{let p=xa(R)??R,y=p.x,b=p.isFixed?p.y:p.y-Nr;return(0,V.jsx)(V.Fragment,{children:(0,V.jsx)(cs,{ref:Hu,element:R.element,selectedText:R.selectedText,screenshot:Ps?.id===R.id?Ps.src:void 0,allowEmpty:typeof l=="object"&&!!R.attributes?.[l.attribute],onOpenSource:a&&R.sourceFile?()=>a(R.sourceFile):void 0,computedStyles:R.computedStylesObj,placeholder:typeof l=="object"&&R.attributes?.[l.attribute]?"Add a note (optional)":R.element==="Area selection"?"What should change in this area?":R.isMultiSelect?"Feedback for this group of elements...":"What should change?",onSubmit:E5,onExitComplete:Yu,onCancel:ma,isExiting:Wn,lightMode:!xo,accentColor:R.isMultiSelect?"var(--agentation-color-green)":"var(--agentation-color-accent)",style:{left:Math.max(160,Math.min(window.innerWidth-160,y/100*window.innerWidth)),...b>window.innerHeight-290?{bottom:window.innerHeight-b+20}:{top:b+20}}},R.id)})})()]}),pe&&(0,V.jsx)(V.Fragment,{children:pe.elementBoundingBoxes?.length?ap.length>0?ap.filter(p=>p.isConnected).map((p,y)=>{let b=Ft(p);return(0,V.jsx)("div",{className:`${H.multiSelectOutline} ${H.enter}`,style:{left:b.left,top:b.top,width:b.width,height:b.height}},`edit-multi-live-${y}`)}):pe.elementBoundingBoxes.map((p,y)=>(0,V.jsx)("div",{className:`${H.multiSelectOutline} ${H.enter}`,style:{left:p.x,top:p.y-Nr,width:p.width,height:p.height}},`edit-multi-${y}`)):(()=>{let p=ku&&ku.isConnected?Ft(ku):null,y=p?{x:p.left,y:p.top,width:p.width,height:p.height}:pe.boundingBox?{x:pe.boundingBox.x,y:pe.isFixed?pe.boundingBox.y:pe.boundingBox.y-Nr,width:pe.boundingBox.width,height:pe.boundingBox.height}:null;return y?(0,V.jsx)("div",{className:`${pe.isMultiSelect?H.multiSelectOutline:H.singleSelectOutline} ${H.enter}`,style:{left:y.x,top:y.y,width:y.width,height:y.height,...pe.isMultiSelect?{}:{borderColor:"color-mix(in srgb, var(--agentation-color-accent) 60%, transparent)",backgroundColor:"color-mix(in srgb, var(--agentation-color-accent) 5%, transparent)"}}}):null})()}),Ao&&(0,V.jsxs)(V.Fragment,{children:[(0,V.jsx)("div",{ref:Es,className:H.dragSelection}),(0,V.jsx)("div",{ref:xi,className:H.highlightsContainer})]})]}),(0,V.jsx)(X1,{ref:sa,annotation:Dn?xa(Dn)??pe:null,editing:!!pe,exiting:Hr,restorePreview:bu,scrollY:Nr,lightMode:!xo,onExited:L5,editorProps:Dn?{element:Dn.element,selectedText:Dn.selectedText,screenshot:Dn.screenshot,allowEmpty:typeof l=="object"&&!!Dn.attributes?.[l.attribute],onOpenSource:a&&Dn.sourceFile?()=>a(Dn.sourceFile):void 0,computedStyles:V0(Dn.computedStyles),placeholder:"Edit your feedback...",initialValue:Dn.comment,submitLabel:"Save",onSubmit:M5,onCancel:T5,onDelete:()=>Vu(Dn.id),accentColor:Dn.isMultiSelect?"var(--agentation-color-green)":"var(--agentation-color-accent)"}:void 0})]})]})}var Ok="3.1.2";return U5(Bk);})();
