use crate::types::{ServiceStatus, StatusColor};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3_asyncio::tokio::into_future;
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::RwLock, task::JoinHandle};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Clone)]
pub struct AppState {
    pub health_tree: Arc<RwLock<ServiceStatus>>,
}

pub async fn get_health(State(state): State<AppState>) -> impl IntoResponse {
    let tree = state.health_tree.read().await;
    (StatusCode::OK, Json(tree.clone()))
}

const DASHBOARD_HTML: &str = r###"<!DOCTYPE html><html><head>
<meta charset="utf-8"><title>Medic Dashboard</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
html,body{margin:0;height:100%;font-family:sans-serif;background:#111;color:#eee}
header{padding:8px 16px;font-size:24px;font-weight:600}
#wrap{display:flex;flex-direction:column;height:calc(100vh - 48px)}
#chart{flex:1}
#history{height:200px}
</style></head><body>
<header>🚑 Medic Status</header>
<div id="wrap">
  <div id="chart"></div>
  <div id="history"></div>
</div>
<script>
const endpoint="/health", poll=3000, history=[], maxPts=120;
function color(c){return c==="GREEN"?"#4caf50":c==="ORANGE"?"#ff9800":"#f44336";}
function statusVal(c){return c==="GREEN"?2:c==="ORANGE"?1:0;}
function drawTreemap(data){
 const root=d3.hierarchy(data,d=>d.subservices).sum(()=>1);
 const w=document.getElementById("chart").clientWidth,
       h=document.getElementById("chart").clientHeight;
 d3.treemap().size([w,h]).padding(2)(root);
 const svg=d3.select("#chart").html("").append("svg").attr("width",w).attr("height",h);
 const g=svg.selectAll("g").data(root.descendants()).join("g")
            .attr("transform",d=>`translate(${d.x0},${d.y0})`);
 g.append("rect").attr("width",d=>d.x1-d.x0).attr("height",d=>d.y1-d.y0)
   .attr("fill",d=>color(d.data.status));
 g.append("title").text(d=>`${d.data.name}\n${d.data.status}`);
 g.filter(d=>d.depth===1).append("text").attr("x",4).attr("y",14)
   .text(d=>d.data.name).attr("fill","#fff").attr("font-size",12);
}
function drawHistory(){
 const cont=document.getElementById("history"),
       w=cont.clientWidth,h=cont.clientHeight;
 const svg=d3.select("#history").html("").append("svg").attr("width",w).attr("height",h);
 const x=d3.scaleLinear().domain([Math.max(0,history.length-maxPts),history.length-1]).range([40,w-10]);
 const y=d3.scaleLinear().domain([0,2]).range([h-20,10]);
 const line=d3.line().x((d,i)=>x(i)).y(d=>y(d.v));
 svg.append("path").attr("d",line(history)).attr("fill","none").attr("stroke","#00bcd4").attr("stroke-width",2);
 svg.selectAll("circle").data(history).join("circle")
    .attr("cx",(d,i)=>x(i)).attr("cy",d=>y(d.v)).attr("r",3).attr("fill",d=>color(d.c));
 const ax=d3.axisBottom(x).ticks(5).tickFormat(()=>"");
 const ay=d3.axisLeft(y).ticks(3).tickFormat(d=>d===2?"GREEN":d===1?"ORANGE":"RED");
 svg.append("g").attr("transform",`translate(0,${h-20})`).call(ax);
 svg.append("g").attr("transform","translate(40,0)").call(ay);
}
async function tick(){
 const r=await fetch(endpoint);
 if(r.ok){
   const data=await r.json();
   drawTreemap(data);
   history.push({v:statusVal(data.status),c:data.status});
   if(history.length>maxPts)history.shift();
   drawHistory();
 }}
tick();setInterval(tick,poll);
</script></body></html>"###;

pub async fn get_dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

fn log_py_err(msg: &str, err: PyErr) {
    Python::with_gil(|py| {
        error!("{msg}: {:?}", err);
        err.print(py);
    });
}

pub async fn polling_task(
    py_services: Vec<PyObject>,
    tree: Arc<RwLock<ServiceStatus>>,
    interval: Duration,
) {
    loop {
        let mut sub_statuses = Vec::with_capacity(py_services.len());

        for obj in &py_services {
            let fut_res: PyResult<_> = Python::with_gil(|py| {
                let coro = obj.as_ref(py).call_method0("health")?;
                into_future(coro)
            });

            match fut_res {
                Ok(fut) => match fut.await {
                    Ok(result) => {
                        match Python::with_gil(|py| ServiceStatus::try_from(result.as_ref(py))) {
                            Ok(status) => sub_statuses.push(status),
                            Err(e) => log_py_err("extract ServiceStatus failed", e),
                        }
                    }
                    Err(e) => log_py_err("health() raised", e),
                },
                Err(e) => log_py_err("into_future() failed", e),
            }
        }

        let global_status = if sub_statuses
            .iter()
            .all(|s| matches!(s.status, StatusColor::Green))
        {
            StatusColor::Green
        } else if sub_statuses
            .iter()
            .any(|s| matches!(s.status, StatusColor::Red))
        {
            StatusColor::Red
        } else {
            StatusColor::Orange
        };

        *tree.write().await = ServiceStatus {
            name: "medic".into(),
            status: global_status,
            description: None,
            subservices: sub_statuses,
        };

        tokio::time::sleep(interval).await;
    }
}

#[pyfunction]
pub fn set_probe(py: Python<'_>, services: Vec<PyObject>) -> PyResult<()> {
    tracing::subscriber::set_global_default(
        FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish(),
    )
    .map_err(|e| PyRuntimeError::new_err(format!("failed to init tracing: {e}")))?;

    pyo3_asyncio::tokio::run(py, async move {
        let tree = Arc::new(RwLock::new(ServiceStatus {
            name: "medic".into(),
            status: StatusColor::Orange,
            description: Some("warming up".into()),
            subservices: vec![],
        }));

        let task_locals = Python::with_gil(|py| pyo3_asyncio::tokio::get_current_locals(py))?;

        let _bg: JoinHandle<()> = tokio::spawn(pyo3_asyncio::tokio::scope(
            task_locals,
            polling_task(services, tree.clone(), Duration::from_secs(5)),
        ));

        let state = AppState { health_tree: tree };

        let app = Router::new()
            .route("/health", get(get_health))
            .route("/", get(get_dashboard))
            .with_state(state);

        let listener = TcpListener::bind("0.0.0.0:3000").await?;
        info!("Medic server at http://{}", listener.local_addr()?);
        axum::serve(listener, app).await?;
        Ok(())
    })
}
