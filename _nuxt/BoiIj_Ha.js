import{Ci as e,ji as t,st as n,vi as r}from"#entry";import{t as i}from"./D_p6QMA0.js";var a=Object.assign(e({__name:`PmInstall`,props:{inStack:{type:Boolean,default:!1},name:{},sync:{default:`_pm`},saveDev:{type:Boolean,default:!1},noSync:{type:Boolean}},setup(e){let a=`
::code-group{${e.inStack?`in-stack`:``} ${e.noSync?``:`sync="${e.sync}"`}}
${i().packageManagers.value.map(t=>{let n=e.name?`${t.command}${t.install}${e.saveDev?t.saveDev:``}${e.name}`:`${t.command}${t.installEmpty}`;return`\`\`\`bash [${t.name}]\n${n}\n\`\`\`\n`}).join(`
`)}
::
`;return(e,i)=>{let o=n;return t(),r(o,{value:a,class:`[&:not(:first-child)]:mt-5`})}}}),{__name:`PmInstall`});export{a as default};