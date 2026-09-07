import {EditorState, EditorSelection} from '@codemirror/state';
import {indentOnInput, indentUnit, StreamLanguage, ensureSyntaxTree} from '@codemirror/language';
import {insertNewlineAndIndent, indentMore, indentLess} from '@codemirror/commands';
import {javascript} from '@codemirror/lang-javascript';
import {python} from '@codemirror/lang-python';
import {yaml} from '@codemirror/lang-yaml';
import {html} from '@codemirror/lang-html';
import {json} from '@codemirror/lang-json';
import {dart} from '@codemirror/legacy-modes/mode/clike';
import {ruby} from '@codemirror/legacy-modes/mode/ruby';
import {shell} from '@codemirror/legacy-modes/mode/shell';
import fs from 'node:fs';
const languages={dart:()=>StreamLanguage.define(dart),javascript,typescript:()=>javascript({typescript:true}),python,yaml,html,json,ruby:()=>StreamLanguage.define(ruby),shell:()=>StreamLanguage.define(shell)};
const cases=[
 ['dart owner','dart','for (final x in [1,2,3]) {',[['enter'],['type','}']]],
 ['javascript nested','javascript','function run() {\n  if (ok) {',[['enter'],['type','}'],['enter'],['type','}']]],
 ['typescript block','typescript','function run(): void {',[['enter'],['type','}']]],
 ['javascript regex','javascript','function run() {\n  const re = /[}]/;',[['enter'],['type','}']]],
 ['javascript open comment','javascript','function run() {\n  /*',[['enter'],['type','}']]],
 ['javascript template expression','javascript','const s = `${(() => {',[['enter'],['type','}']]],
 ['javascript pasted closer','javascript','function run() {\n  ',[['paste','}']]],
 ['python suite','python','if ready:',[['enter'],['type','pass'],['enter'],['type','else:'],['enter']]],
 ['python bracket','python','values = [',[['enter'],['type',']']]],
 ['yaml mapping','yaml','settings:',[['enter'],['type','enabled: true']]],
 ['yaml flow','yaml','settings: {',[['enter'],['type','}']]],
 ['html element','html','<div>',[['enter'],['type','</div>']]],
 ['json object','json','{',[['enter'],['type','}']]],
 ['ruby keyword','ruby','def run',[['enter'],['type','end']]],
 ['shell keyword','shell','if true; then',[['enter'],['type','fi']]],
 ['dart indent round trip','dart','void main() {\n  print(42);',[['indent'],['outdent']]],
];
const results=[];
for(const [name,language,doc,actions] of cases){
 let state=EditorState.create({doc,selection:{anchor:doc.length},extensions:[languages[language](),indentUnit.of(language==='python'?'    ':'  '),indentOnInput()]});
 const steps=[];
 for(const [action,text] of actions){
  ensureSyntaxTree(state,state.doc.length,1000);
  if(action==='enter'||action==='indent'||action==='outdent'){
   const command={enter:insertNewlineAndIndent,indent:indentMore,outdent:indentLess}[action];
   command({state,dispatch:tr=>state=tr.state});
  }else{
   // Feed each typed character, as an editor does; paste is one literal transaction.
   for(const chunk of action==='paste'?[text]:[...text]){
    const {from,to}=state.selection.main;
    state=state.update({changes:{from,to,insert:chunk},selection:EditorSelection.cursor(from+chunk.length),userEvent:action==='paste'?'input.paste':'input.type'}).state;
    ensureSyntaxTree(state,state.doc.length,1000);
   }
  }
  steps.push({action,text:state.doc.toString(),caret:state.selection.main.head});
 }
 results.push({name,language,before:doc,steps});
}
const lock=JSON.parse(fs.readFileSync(new URL('./package-lock.json',import.meta.url)));
const report={runtime:process.version,method:'Headless EditorState language and command probe; no DOM, Flutter adapter, paint, IME, latency or deployment claim.',packages:Object.fromEntries(Object.entries(lock.packages).filter(([p])=>p.startsWith('node_modules/@codemirror/')).map(([p,v])=>[p.slice(13),v.version])),results};
console.log(JSON.stringify(report,null,2));
