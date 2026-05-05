import{d as c,a as o,o as l,bJ as m}from"./D1CCrpmu.js";import{u as i}from"./B3NyyKHY.js";const u=c({__name:"PmInstall",props:{inStack:{type:Boolean,default:!1},name:{},sync:{default:"_pm"},saveDev:{type:Boolean,default:!1},noSync:{type:Boolean}},setup(n){const t=`
::code-group{${n.inStack?"in-stack":""} ${n.noSync?"":`sync="${n.sync}"`}}
${i().packageManagers.value.map(a=>{const e=n.name?`${a.command}${a.install}${n.saveDev?a.saveDev:""}${n.name}`:`${a.command}${a.installEmpty}`;return`\`\`\`bash [${a.name}]
${e}
\`\`\`
`}).join(`
`)}
::
`;return(a,e)=>{const s=m;return l(),o(s,{value:t,class:"[&:not(:first-child)]:mt-5"})}}}),f=Object.assign(u,{__name:"PmInstall"});export{f as default};
