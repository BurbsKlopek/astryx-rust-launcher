import{r as l,j as t}from"./react-vendor-pcoHEaBX.js";import{c as s,f as j,b as p}from"./index-CZSU9mXo.js";import{H as f,T as y,B as d,D as k}from"./astryx-vendor-ybD1A7tp.js";import{T as N}from"./trash-2-DuDfeeO0.js";import{X as g}from"./x-Co1RaqUy.js";import"./a2ui-vendor-DwfoA5PQ.js";/**
 * @license lucide-react v0.511.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const z=[["path",{d:"m12 19-7-7 7-7",key:"1l729n"}],["path",{d:"M19 12H5",key:"x3x0zl"}]],_=s("arrow-left",z);/**
 * @license lucide-react v0.511.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const C=[["path",{d:"M5 12h14",key:"1ays0h"}],["path",{d:"m12 5 7 7-7 7",key:"xquz4c"}]],M=s("arrow-right",C);/**
 * @license lucide-react v0.511.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const w=[["path",{d:"M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49",key:"ct8e1f"}],["path",{d:"M14.084 14.158a3 3 0 0 1-4.242-4.242",key:"151rxh"}],["path",{d:"M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143",key:"13bj9a"}],["path",{d:"m2 2 20 20",key:"1ooewy"}]],A=s("eye-off",w);/**
 * @license lucide-react v0.511.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const $=[["circle",{cx:"12",cy:"12",r:"10",key:"1mglay"}],["path",{d:"M12 16v-4",key:"1dtifu"}],["path",{d:"M12 8h.01",key:"e9boi3"}]],H=s("info",$);/**
 * @license lucide-react v0.511.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const E=[["path",{d:"M12 17v5",key:"bb1du9"}],["path",{d:"M15 9.34V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H7.89",key:"znwnzq"}],["path",{d:"m2 2 20 20",key:"1ooewy"}],["path",{d:"M9 9v1.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h11",key:"c9qhm2"}]],I=s("pin-off",E);/**
 * @license lucide-react v0.511.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const P=[["polygon",{points:"6 3 20 12 6 21 6 3",key:"1oa8hb"}]],R=s("play",P),D=({app:e,isPinned:n,canMoveLeft:o,canMoveRight:c,onLaunch:x,onTogglePin:h,onMoveFavorite:r,onHideApp:u,onOpenInfo:b,onUninstall:m,onClose:i})=>{const a=l.useRef(null);return l.useEffect(()=>{a.current&&j(a.current)},[]),t.jsx("div",{className:"tv-modal-backdrop",onClick:i,children:t.jsxs("div",{ref:a,className:"tv-context-sheet glass-panel-strong","data-tv-modal":"true",onClick:v=>v.stopPropagation(),children:[t.jsxs("div",{className:"tv-context-head",children:[t.jsx("img",{src:e.iconUrl||e.bannerUrl,alt:e.label,className:"tv-context-thumb"}),t.jsxs("div",{children:[t.jsx(f,{level:3,children:e.label}),t.jsx(y,{size:"sm",color:"secondary",children:e.packageName}),t.jsxs("div",{className:"tv-context-badges",children:[t.jsx(d,{label:e.category,variant:"info"}),e.versionName&&t.jsx(d,{label:`v${e.versionName}`,variant:"neutral"})]})]})]}),t.jsx(k,{}),t.jsxs("div",{className:"tv-context-actions",children:[t.jsxs("button",{type:"button",className:"tv-context-btn","data-tv-focusable":"true",onClick:x,children:[t.jsx(R,{size:18}),t.jsxs("span",{children:["Open ",e.label]})]}),t.jsxs("button",{type:"button",className:"tv-context-btn","data-tv-focusable":"true",onClick:h,children:[n?t.jsx(I,{size:18}):t.jsx(p,{size:18}),t.jsx("span",{children:n?"Unpin from Favorites":"Pin to Favorites"})]}),n&&(o||c)&&t.jsxs("div",{className:"tv-reorder-row",children:[o&&t.jsxs("button",{type:"button",className:"tv-context-btn","data-tv-focusable":"true",onClick:()=>r(-1),children:[t.jsx(_,{size:16}),t.jsx("span",{children:"Move Left"})]}),c&&t.jsxs("button",{type:"button",className:"tv-context-btn","data-tv-focusable":"true",onClick:()=>r(1),children:[t.jsx(M,{size:16}),t.jsx("span",{children:"Move Right"})]})]}),t.jsxs("button",{type:"button",className:"tv-context-btn","data-tv-focusable":"true",onClick:u,children:[t.jsx(A,{size:18}),t.jsx("span",{children:"Hide from Launcher"})]}),t.jsxs("button",{type:"button",className:"tv-context-btn","data-tv-focusable":"true",onClick:b,children:[t.jsx(H,{size:18}),t.jsx("span",{children:"App Info & Permissions"})]}),!e.isSystemApp&&t.jsxs("button",{type:"button",className:"tv-context-btn tv-context-danger","data-tv-focusable":"true",onClick:m,children:[t.jsx(N,{size:18}),t.jsx("span",{children:"Uninstall App"})]}),t.jsxs("button",{type:"button",className:"tv-context-btn","data-tv-focusable":"true",onClick:i,children:[t.jsx(g,{size:18}),t.jsx("span",{children:"Cancel"})]})]})]})})};export{D as AppContextMenu};
