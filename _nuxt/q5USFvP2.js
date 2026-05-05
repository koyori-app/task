import{av as S,an as R,cm as q,H as Q,G as Y,I as tt,J as et,a2 as at,a1 as rt,K as p,N as W,L as nt,ag as it,aj as st,aC as ot,O as lt,a8 as ct,ah as ut}from"./CySWvUbY.js";import{p as pt}from"./Chv-oPyh.js";import{p as dt}from"./aOkVx0pn.js";import{d as P}from"./BNMQQ-RP.js";import{o as gt}from"./CmKTTxBW.js";import"./nbwKliOf.js";import"./BQ9AFeEs.js";import"./Gi6I4Gst.js";function ft(t,a){return a<t?-1:a>t?1:a>=t?0:NaN}function ht(t){return t}function mt(){var t=ht,a=ft,f=null,y=S(0),s=S(R),d=S(0);function o(e){var n,l=(e=q(e)).length,g,h,v=0,c=new Array(l),i=new Array(l),x=+y.apply(this,arguments),w=Math.min(R,Math.max(-R,s.apply(this,arguments)-x)),m,D=Math.min(Math.abs(w)/l,d.apply(this,arguments)),$=D*(w<0?-1:1),u;for(n=0;n<l;++n)(u=i[c[n]=n]=+t(e[n],n,e))>0&&(v+=u);for(a!=null?c.sort(function(A,C){return a(i[A],i[C])}):f!=null&&c.sort(function(A,C){return f(e[A],e[C])}),n=0,h=v?(w-l*$)/v:0;n<l;++n,x=m)g=c[n],u=i[g],m=x+(u>0?u*h:0)+$,i[g]={data:e[g],index:n,value:u,startAngle:x,endAngle:m,padAngle:D};return i}return o.value=function(e){return arguments.length?(t=typeof e=="function"?e:S(+e),o):t},o.sortValues=function(e){return arguments.length?(a=e,f=null,o):a},o.sort=function(e){return arguments.length?(f=e,a=null,o):f},o.startAngle=function(e){return arguments.length?(y=typeof e=="function"?e:S(+e),o):y},o.endAngle=function(e){return arguments.length?(s=typeof e=="function"?e:S(+e),o):s},o.padAngle=function(e){return arguments.length?(d=typeof e=="function"?e:S(+e),o):d},o}var vt=ut.pie,z={sections:new Map,showData:!1},T=z.sections,F=z.showData,xt=structuredClone(vt),St=p(()=>structuredClone(xt),"getConfig"),yt=p(()=>{T=new Map,F=z.showData,ct()},"clear"),wt=p(({label:t,value:a})=>{if(a<0)throw new Error(`"${t}" has invalid value: ${a}. Negative values are not allowed in pie charts. All slice values must be >= 0.`);T.has(t)||(T.set(t,a),W.debug(`added new section: ${t}, with value: ${a}`))},"addSection"),At=p(()=>T,"getSections"),Ct=p(t=>{F=t},"setShowData"),Dt=p(()=>F,"getShowData"),_={getConfig:St,clear:yt,setDiagramTitle:rt,getDiagramTitle:at,setAccTitle:et,getAccTitle:tt,setAccDescription:Y,getAccDescription:Q,addSection:wt,getSections:At,setShowData:Ct,getShowData:Dt},$t=p((t,a)=>{pt(t,a),a.setShowData(t.showData),t.sections.map(a.addSection)},"populateDb"),Tt={parse:p(async t=>{const a=await dt("pie",t);W.debug(a),$t(a,_)},"parse")},kt=p(t=>`
  .pieCircle{
    stroke: ${t.pieStrokeColor};
    stroke-width : ${t.pieStrokeWidth};
    opacity : ${t.pieOpacity};
  }
  .pieOuterCircle{
    stroke: ${t.pieOuterStrokeColor};
    stroke-width: ${t.pieOuterStrokeWidth};
    fill: none;
  }
  .pieTitleText {
    text-anchor: middle;
    font-size: ${t.pieTitleTextSize};
    fill: ${t.pieTitleTextColor};
    font-family: ${t.fontFamily};
  }
  .slice {
    font-family: ${t.fontFamily};
    fill: ${t.pieSectionTextColor};
    font-size:${t.pieSectionTextSize};
    // fill: white;
  }
  .legend text {
    fill: ${t.pieLegendTextColor};
    font-family: ${t.fontFamily};
    font-size: ${t.pieLegendTextSize};
  }
`,"getStyles"),Et=kt,Mt=p(t=>{const a=[...t.values()].reduce((s,d)=>s+d,0),f=[...t.entries()].map(([s,d])=>({label:s,value:d})).filter(s=>s.value/a*100>=1);return mt().value(s=>s.value).sort(null)(f)},"createPieArcs"),bt=p((t,a,f,y)=>{W.debug(`rendering pie chart
`+t);const s=y.db,d=nt(),o=it(s.getConfig(),d.pie),e=40,n=18,l=4,g=450,h=g,v=st(a),c=v.append("g");c.attr("transform","translate("+h/2+","+g/2+")");const{themeVariables:i}=d;let[x]=ot(i.pieOuterStrokeWidth);x??=2;const w=o.textPosition,m=Math.min(h,g)/2-e,D=P().innerRadius(0).outerRadius(m),$=P().innerRadius(m*w).outerRadius(m*w);c.append("circle").attr("cx",0).attr("cy",0).attr("r",m+x/2).attr("class","pieOuterCircle");const u=s.getSections(),A=Mt(u),C=[i.pie1,i.pie2,i.pie3,i.pie4,i.pie5,i.pie6,i.pie7,i.pie8,i.pie9,i.pie10,i.pie11,i.pie12];let k=0;u.forEach(r=>{k+=r});const G=A.filter(r=>(r.data.value/k*100).toFixed(0)!=="0"),E=gt(C).domain([...u.keys()]);c.selectAll("mySlices").data(G).enter().append("path").attr("d",D).attr("fill",r=>E(r.data.label)).attr("class","pieCircle"),c.selectAll("mySlices").data(G).enter().append("text").text(r=>(r.data.value/k*100).toFixed(0)+"%").attr("transform",r=>"translate("+$.centroid(r)+")").style("text-anchor","middle").attr("class","slice");const V=c.append("text").text(s.getDiagramTitle()).attr("x",0).attr("y",-400/2).attr("class","pieTitleText"),L=[...u.entries()].map(([r,b])=>({label:r,value:b})),M=c.selectAll(".legend").data(L).enter().append("g").attr("class","legend").attr("transform",(r,b)=>{const I=n+l,K=I*L.length/2,X=12*n,Z=b*I-K;return"translate("+X+","+Z+")"});M.append("rect").attr("width",n).attr("height",n).style("fill",r=>E(r.label)).style("stroke",r=>E(r.label)),M.append("text").attr("x",n+l).attr("y",n-l).text(r=>s.getShowData()?`${r.label} [${r.value}]`:r.label);const j=Math.max(...M.selectAll("text").nodes().map(r=>r?.getBoundingClientRect().width??0)),U=h+e+n+l+j,N=V.node()?.getBoundingClientRect().width??0,H=h/2-N/2,J=h/2+N/2,O=Math.min(0,H),B=Math.max(U,J)-O;v.attr("viewBox",`${O} 0 ${B} ${g}`),lt(v,g,B,o.useMaxWidth)},"draw"),Rt={draw:bt},Pt={parser:Tt,db:_,renderer:Rt,styles:Et};export{Pt as diagram};
