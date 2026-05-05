import{Ci as e,ji as t,st as n,vi as r}from"#entry";import{t as i}from"./D_p6QMA0.js";var a=Object.assign(e({__name:`PmRun`,props:{inStack:{type:Boolean,default:!1},script:{},sync:{default:`_pm`},noSync:{type:Boolean}},setup(e){let a=`
::code-group{${e.inStack?`in-stack`:``} ${e.noSync?``:`sync="${e.sync}"`}}
${i().packageManagers.value.map(t=>{let n=`${t.command}${t.run}${e.script}`;return`\`\`\`bash [${t.name}]\n${n}\n\`\`\`\n`}).join(`
`)}
::
`;return(e,i)=>{let o=n;return t(),r(o,{value:a,class:`[&:not(:first-child)]:mt-5`})}}}),{__name:`PmRun`});export{a as default};