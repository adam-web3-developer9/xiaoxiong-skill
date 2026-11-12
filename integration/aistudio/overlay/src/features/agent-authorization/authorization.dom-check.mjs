// Focused DOM check. Run with Node >=24 in an AIStudio checkout with dependencies installed.
// Only the native host and overlay coordinator are isolated; AppDrawer/antd are real.
import { registerHooks } from 'node:module';
import { readFileSync } from 'node:fs';
import { existsSync } from 'node:fs';
import ts from 'typescript';
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.endsWith('/overlays/OverlayProvider')) return { url: 'data:text/javascript,export function useOverlay() {}', shortCircuit: true };
    if (specifier.startsWith('.') && context.parentURL?.startsWith('file:')) {
      for (const extension of ['', '.ts', '.tsx']) {
        const url = new URL(specifier + extension, context.parentURL);
        if (/\.tsx?$/.test(url.pathname) && existsSync(url)) return { url: url.href, shortCircuit: true };
      }
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url.startsWith('file:') && /\.tsx?$/.test(new URL(url).pathname)) return {
      format: 'module', shortCircuit: true,
      source: ts.transpileModule(readFileSync(new URL(url), 'utf8'), { compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
    };
    return nextLoad(url, context);
  }
});
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://localhost' });
for (const key of ['window','document','HTMLElement','Element','Node','ShadowRoot','SVGElement','MutationObserver','getComputedStyle']) Object.defineProperty(globalThis,key,{configurable:true,value: key === 'getComputedStyle' ? dom.window.getComputedStyle.bind(dom.window) : dom.window[key]});
Object.defineProperty(globalThis, 'navigator', { configurable:true, value:dom.window.navigator });
window.matchMedia = () => ({ matches:false, addListener(){}, removeListener(){}, addEventListener(){}, removeEventListener(){}, dispatchEvent(){return false;} });
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.isTauri = true;
const calls=[];
const request={requestId:'r1',client:{id:'test',name:'Test OpenCode',version:'1'},scopes:['edit','channels.credentials.write'],draftIds:['draft-one'],userCode:'ABCD-EFGH',expiresAt:Date.now()+60000,status:'pending'};
let snapshot={pending:[request],grants:[]};
let finishApprove;
window.__TAURI_INTERNALS__={invoke: async (cmd,args)=>{assert.equal(cmd,'agent_authorization');calls.push(args);if(args.action==='approve'){await new Promise(resolve=>finishApprove=resolve);snapshot={pending:[],grants:[{sessionId:'s1',client:request.client,scopes:request.scopes,draftIds:request.draftIds,expiresAt:request.expiresAt}]};}if(args.action==='revoke')snapshot={pending:[],grants:[]};return snapshot;}};
const React=await import('react');
const {createRoot}=await import('react-dom/client');
const {AgentAuthorizationPanel}=await import('./AgentAuthorizationPanel.tsx');
const root=createRoot(document.getElementById('root'));
const settle=()=>new Promise(resolve=>setTimeout(resolve,30));
const button=(text)=>[...document.querySelectorAll('button')].find(x=>x.textContent?.includes(text));
await React.act(async()=>{root.render(React.createElement(AgentAuthorizationPanel));await settle();});
assert.match(document.body.textContent,/ABCD-EFGH/);
assert.match(document.body.textContent,/仅写入，不能读取已有私钥/);
assert.ok(document.querySelector('.ant-drawer-open'));
await React.act(async()=>{document.querySelector('.ant-drawer-close').click();await settle();});
assert.equal(document.querySelector('.ant-drawer-open'),null);
await React.act(async()=>{button('外部工具').click();await settle();});
await React.act(async()=>{button('允许连接').click();button('允许连接').click();await settle();});
assert.equal(calls.filter(x=>x.action==='approve').length,1);
assert.deepEqual(calls.find(x=>x.action==='approve'),{action:'approve',requestId:'r1'});
await React.act(async()=>{finishApprove();await settle();});
assert.match(document.body.textContent,/已授权，请返回终端继续/);
await React.act(async()=>{button('撤销授权').click();await settle();});
assert.deepEqual(calls.find(x=>x.action==='revoke'),{action:'revoke',sessionId:'s1'});
snapshot={pending:[{...request,requestId:'expired',expiresAt:Date.now()-1}],grants:[]};
await React.act(async()=>{button('外部工具').click();await settle();});
assert.equal(button('允许连接').disabled,true);
await React.act(async()=>{root.unmount();await settle();});
console.log('PASS real antd DOM: auto-open, safe disclosure, dismiss, reopen, double-click approval, grant, revoke, expired button');
