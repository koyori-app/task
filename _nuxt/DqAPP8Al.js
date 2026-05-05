import{t as e}from"./CjVRjOny.js";import{n as t}from"./B7t5W_i1.js";import{t as n}from"./CQ55Gnro.js";import{t as r}from"./CSG9mMKd.js";import{$n as i,Fr as a,Gt as o,Ln as s,Pr as c,Sn as l,Sr as u,Xt as d,ar as f,br as p,cr as m,ir as h,lr as g,nr as _,or as v,yr as y}from"#entry";import{t as b}from"./C3N9liMK2.js";import{t as x}from"./BMY9Ds5q2.js";function S(e,t){return t<e?-1:t>e?1:t>=e?0:NaN}function C(e){return e}function w(){var e=C,n=S,i=null,a=t(0),o=t(s),c=t(0);function l(t){var l,u=(t=r(t)).length,d,f,p=0,m=Array(u),h=Array(u),g=+a.apply(this,arguments),_=Math.min(s,Math.max(-s,o.apply(this,arguments)-g)),v,y=Math.min(Math.abs(_)/u,c.apply(this,arguments)),b=y*(_<0?-1:1),x;for(l=0;l<u;++l)(x=h[m[l]=l]=+e(t[l],l,t))>0&&(p+=x);for(n==null?i!=null&&m.sort(function(e,n){return i(t[e],t[n])}):m.sort(function(e,t){return n(h[e],h[t])}),l=0,f=p?(_-u*b)/p:0;l<u;++l,g=v)d=m[l],x=h[d],v=g+(x>0?x*f:0)+b,h[d]={data:t[d],index:l,value:x,startAngle:g,endAngle:v,padAngle:y};return h}return l.value=function(n){return arguments.length?(e=typeof n==`function`?n:t(+n),l):e},l.sortValues=function(e){return arguments.length?(n=e,i=null,l):n},l.sort=function(e){return arguments.length?(i=e,n=null,l):i},l.startAngle=function(e){return arguments.length?(a=typeof e==`function`?e:t(+e),l):a},l.endAngle=function(e){return arguments.length?(o=typeof e==`function`?e:t(+e),l):o},l.padAngle=function(e){return arguments.length?(c=typeof e==`function`?e:t(+e),l):c},l}var T=h.pie,E={sections:new Map,showData:!1,config:T},D=E.sections,O=E.showData,k=structuredClone(T),A={getConfig:c(()=>structuredClone(k),`getConfig`),clear:c(()=>{D=new Map,O=E.showData,i()},`clear`),setDiagramTitle:u,getDiagramTitle:g,setAccTitle:p,getAccTitle:v,setAccDescription:y,getAccDescription:f,addSection:c(({label:e,value:t})=>{if(t<0)throw Error(`"${e}" has invalid value: ${t}. Negative values are not allowed in pie charts. All slice values must be >= 0.`);D.has(e)||(D.set(e,t),a.debug(`added new section: ${e}, with value: ${t}`))},`addSection`),getSections:c(()=>D,`getSections`),setShowData:c(e=>{O=e},`setShowData`),getShowData:c(()=>O,`getShowData`)},j=c((e,t)=>{b(e,t),t.setShowData(e.showData),e.sections.map(t.addSection)},`populateDb`),M={parse:c(async e=>{let t=await x(`pie`,e);a.debug(t),j(t,A)},`parse`)},N=c(e=>`
  .pieCircle{
    stroke: ${e.pieStrokeColor};
    stroke-width : ${e.pieStrokeWidth};
    opacity : ${e.pieOpacity};
  }
  .pieOuterCircle{
    stroke: ${e.pieOuterStrokeColor};
    stroke-width: ${e.pieOuterStrokeWidth};
    fill: none;
  }
  .pieTitleText {
    text-anchor: middle;
    font-size: ${e.pieTitleTextSize};
    fill: ${e.pieTitleTextColor};
    font-family: ${e.fontFamily};
  }
  .slice {
    font-family: ${e.fontFamily};
    fill: ${e.pieSectionTextColor};
    font-size:${e.pieSectionTextSize};
    // fill: white;
  }
  .legend text {
    fill: ${e.pieLegendTextColor};
    font-family: ${e.fontFamily};
    font-size: ${e.pieLegendTextSize};
  }
`,`getStyles`),P=c(e=>{let t=[...e.values()].reduce((e,t)=>e+t,0),n=[...e.entries()].map(([e,t])=>({label:e,value:t})).filter(e=>e.value/t*100>=1);return w().value(e=>e.value).sort(null)(n)},`createPieArcs`),F={parser:M,db:A,renderer:{draw:c((t,r,i,s)=>{a.debug(`rendering pie chart
`+t);let c=s.db,u=m(),f=o(c.getConfig(),u.pie),p=l(r),h=p.append(`g`);h.attr(`transform`,`translate(225,225)`);let{themeVariables:g}=u,[v]=d(g.pieOuterStrokeWidth);v??=2;let y=f.textPosition,b=n().innerRadius(0).outerRadius(185),x=n().innerRadius(185*y).outerRadius(185*y);h.append(`circle`).attr(`cx`,0).attr(`cy`,0).attr(`r`,185+v/2).attr(`class`,`pieOuterCircle`);let S=c.getSections(),C=P(S),w=[g.pie1,g.pie2,g.pie3,g.pie4,g.pie5,g.pie6,g.pie7,g.pie8,g.pie9,g.pie10,g.pie11,g.pie12],T=0;S.forEach(e=>{T+=e});let E=C.filter(e=>(e.data.value/T*100).toFixed(0)!==`0`),D=e(w).domain([...S.keys()]);h.selectAll(`mySlices`).data(E).enter().append(`path`).attr(`d`,b).attr(`fill`,e=>D(e.data.label)).attr(`class`,`pieCircle`),h.selectAll(`mySlices`).data(E).enter().append(`text`).text(e=>(e.data.value/T*100).toFixed(0)+`%`).attr(`transform`,e=>`translate(`+x.centroid(e)+`)`).style(`text-anchor`,`middle`).attr(`class`,`slice`);let O=h.append(`text`).text(c.getDiagramTitle()).attr(`x`,0).attr(`y`,-400/2).attr(`class`,`pieTitleText`),k=[...S.entries()].map(([e,t])=>({label:e,value:t})),A=h.selectAll(`.legend`).data(k).enter().append(`g`).attr(`class`,`legend`).attr(`transform`,(e,t)=>{let n=22*k.length/2;return`translate(216,`+(t*22-n)+`)`});A.append(`rect`).attr(`width`,18).attr(`height`,18).style(`fill`,e=>D(e.label)).style(`stroke`,e=>D(e.label)),A.append(`text`).attr(`x`,22).attr(`y`,14).text(e=>c.getShowData()?`${e.label} [${e.value}]`:e.label);let j=512+Math.max(...A.selectAll(`text`).nodes().map(e=>e?.getBoundingClientRect().width??0)),M=O.node()?.getBoundingClientRect().width??0,N=450/2-M/2,F=450/2+M/2,I=Math.min(0,N),L=Math.max(j,F)-I;p.attr(`viewBox`,`${I} 0 ${L} 450`),_(p,450,L,f.useMaxWidth)},`draw`)},styles:N};export{F as diagram};