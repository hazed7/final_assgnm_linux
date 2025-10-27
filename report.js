const CODE_MONITOR_MEM=`fn collect_memory_metrics() -> Result<MemoryMetrics, MonitorError> {
	let content=fs::read_to_string("/proc/meminfo")
		.map_err(|e| MonitorError::FileRead(format!("/proc/meminfo: {}",e)))?;
	let mut total=0u64; let mut available=0u64; let mut free=0u64;
	let mut buffers=0u64; let mut cached=0u64;
	for line in content.lines(){
		let parts:Vec<&str>=line.split_whitespace().collect();
		if parts.len()<2{continue;}
		let value=parts[1].parse::<u64>().unwrap_or(0);
		match parts[0]{
			"MemTotal:"=>total=value,
			"MemAvailable:"=>available=value,
			"MemFree:"=>free=value,
			"Buffers:"=>buffers=value,
			"Cached:"=>cached=value,
			_=>{}
		}
	}
	let used=total.saturating_sub(available);
	let used_percent=if total>0{(used as f64/total as f64)*100.0}else{0.0};
	Ok(MemoryMetrics{total_kb:total,available_kb:available,used_kb:used,
		used_percent,free_kb:free,buffers_kb:buffers,cached_kb:cached})
}`;
const CODE_MONITOR_CPU=`static mut PREV_CPU_STATS:Option<(u64,u64,u64,u64)>=None;
fn collect_cpu_metrics()->Result<CpuMetrics,MonitorError>{
	let content=fs::read_to_string("/proc/stat")
		.map_err(|e|MonitorError::FileRead(format!("/proc/stat: {}",e)))?;
	let line=content.lines().next()
		.ok_or_else(||MonitorError::ParseError("Empty /proc/stat".into()))?;
	let parts:Vec<&str>=line.split_whitespace().collect();
	let user:u64=parts[1].parse().unwrap_or(0);
	let system:u64=parts[3].parse().unwrap_or(0);
	let idle:u64=parts[4].parse().unwrap_or(0);
	let iowait:u64=parts.get(5).and_then(|s|s.parse().ok()).unwrap_or(0);
	let total:u64=parts[1..].iter().filter_map(|s|s.parse::<u64>().ok()).sum::<u64>();
	let cpu_usage_percent=unsafe{
		if let Some((pu,ps,_pi,pt))=PREV_CPU_STATS{
			let user_delta=user.saturating_sub(pu);
			let system_delta=system.saturating_sub(ps);
			let total_delta=total.saturating_sub(pt);
			if total_delta>0{((user_delta+system_delta) as f64/total_delta as f64)*100.0}else{0.0}
		}else{0.0}
	};
	unsafe{PREV_CPU_STATS=Some((user,system,idle,total));}
	Ok(CpuMetrics{user_time:user,system_time:system,idle_time:idle,
		iowait_time:iowait,total_time:total,cpu_usage_percent})
}
fn collect_load_metrics()->Result<LoadMetrics,MonitorError>{
	let content=fs::read_to_string("/proc/loadavg")
		.map_err(|e|MonitorError::FileRead(format!("/proc/loadavg: {}",e)))?;
	let parts:Vec<&str>=content.split_whitespace().collect();
	let load_1=parts[0].parse::<f64>()?;
	let load_5=parts[1].parse::<f64>()?;
	let load_15=parts[2].parse::<f64>()?;
	let cpu_count=num_cpus();
	let load_percent_1min=if cpu_count>0{(load_1/cpu_count as f64)*100.0}else{0.0};
	Ok(LoadMetrics{load_average_1min:load_1,load_average_5min:load_5,
		load_average_15min:load_15,cpu_count,load_percent_1min})
}`;
const CODE_MONITOR_SWAP=`fn collect_swap_metrics()->Result<SwapMetrics,MonitorError>{
	let content=fs::read_to_string("/proc/meminfo")
		.map_err(|e|MonitorError::FileRead(format!("/proc/meminfo: {}",e)))?;
	let mut total=0u64; let mut free=0u64;
	for line in content.lines(){
		let parts:Vec<&str>=line.split_whitespace().collect();
		if parts.len()<2{continue;}
		let value=parts[1].parse::<u64>().unwrap_or(0);
		match parts[0]{
			"SwapTotal:"=>total=value,
			"SwapFree:"=>free=value,
			_=>{}
		}
	}
	let used=total.saturating_sub(free);
	let used_percent=if total>0{(used as f64/total as f64)*100.0}else{0.0};
	Ok(SwapMetrics{total_kb:total,used_kb:used,free_kb:free,used_percent})
}`;
const CODE_REPORT=`impl FinalReport{
	fn calc_stats(values:&[f64])->ResourceStats{
		if values.is_empty(){return ResourceStats{min:0.0,max:0.0,avg:0.0,final_value:0.0};}
		let min=values.iter().cloned().fold(f64::INFINITY,f64::min);
		let max=values.iter().cloned().fold(f64::NEG_INFINITY,f64::max);
		let avg=values.iter().sum::<f64>()/values.len() as f64;
		let final_value=*values.last().unwrap_or(&0.0);
		ResourceStats{min,max,avg,final_value}
	}
}`;
const CODE_LEAK=`pub fn spawn_leak_worker(running:Arc<AtomicBool>,config:crate::config::Config){
	let mut buf:Vec<Vec<u8>>=Vec::new();
	let step_bytes=(config.leak_size_mb as usize).saturating_mul(1024*1024);
	let sleep=Duration::from_secs(config.leak_interval_sec as u64);
	let ps=4096;
	while running.load(Ordering::SeqCst){
		let mut chunk=vec![0u8;step_bytes];
		let len=chunk.len();
		let mut i=0usize;
		while i<len{chunk[i]=1;i+=ps;}
		if len>0{chunk[len-1]=chunk[len-1].wrapping_add(1);}
		LEAK_TOTAL_BYTES.fetch_add(step_bytes as u64,Ordering::Relaxed);
		buf.push(chunk);
		thread::sleep(sleep);
	}
	std::hint::black_box(&buf);
}`;

const f2 = n => (typeof n === "number" ? n.toFixed(2) : n)
const mb = kb => Math.round((kb || 0) / 1024)
const textTime = s => { const t = (s || "").split("T")[1]; return t ? t.slice(0,8) : s }
const badgeEl = (txt, cls) => { const x = document.createElement("span"); x.className = `badge ${cls||""}`.trim(); x.textContent = txt; return x }

const clsMem = v => v > 85 ? "crit" : v > 70 ? "warn" : "ok"
const clsCpu = v => v > 85 ? "crit" : v > 70 ? "warn" : "ok"
const clsLoad = v => v > 250 ? "crit" : v > 150 ? "warn" : "ok"
const clsSwap = v => v > 60 ? "crit" : v > 30 ? "warn" : "ok"

function setCode() {
	const byId = (id, val) => { const el = document.getElementById(id); if (el) el.textContent = val }
	byId("code-monitor-mem",  CODE_MONITOR_MEM)
	byId("code-monitor-cpu",  CODE_MONITOR_CPU)
	byId("code-monitor-swap", CODE_MONITOR_SWAP)
	byId("code-report",       CODE_REPORT)
	byId("code-leak",         CODE_LEAK)
	if (window.Prism && Prism.highlightAll) Prism.highlightAll()
}

function mark(ok) {
	return `<span class="badge ${ok ? "ok" : "crit"}">${ok ? "+" : "-"}</span>`
}

function fillScoreTable() {
	const rows = [
		{ name:"Имитируется утечка памяти", pts:5, ok:true },
		{ name:"Автоматический опрос", pts:5, ok:true },
		{ name:"Память", pts:5, ok:true },
		{ name:"Загрузка CPU", pts:5, ok:true },
		{ name:"Использование swap", pts:5, ok:true },
		{ name:"Критические события", pts:5, ok:true },
		{ name:"Анализ событий ядра / OOM", pts:5, ok:false },
		{ name:"Генерация отчёта", pts:5, ok:true },
		{ name:"Память", pts:5, ok:true },
		{ name:"Загрузка CPU", pts:5, ok:true },
		{ name:"Использование swap", pts:5, ok:true },
		{ name:"Критические события", pts:5, ok:true }
	]
	const body = document.querySelector("#score-tbl tbody")
	if (!body) return
	body.innerHTML = ""
	let sum = 0
	rows.forEach(r => {
		const tr = document.createElement("tr")
		tr.innerHTML = `<td>${r.name}</td><td>${r.pts}</td><td>${mark(r.ok)}</td>`
		body.appendChild(tr)
		if (r.ok) sum += r.pts
	})
	const sumEl = document.getElementById("score-sum")
	if (sumEl) sumEl.textContent = String(sum)
}

function renderSnapshots(snapshots) {
	if (!Array.isArray(snapshots) || snapshots.length === 0) {
		const gen = document.getElementById("generated-at")
		if (gen) gen.textContent = "данные отсутствуют"
		fillScoreTable()
		return
	}

	const lastTs = snapshots[snapshots.length - 1].timestamp || ""
	const gen = document.getElementById("generated-at")
	if (gen) gen.textContent = lastTs.replace("T"," ").split("+")[0] || "—"

	const memBody = document.querySelector("#mem-tbl tbody")
	const memPerc = [], memTot = [], memUsed = [], memAvail = []
	snapshots.forEach(s => {
		const m = s.metrics.memory
		const tr = document.createElement("tr")
		tr.innerHTML = `<td>${s.iteration}</td><td>${textTime(s.timestamp)}</td><td>${f2(m.used_percent)}%</td><td>${mb(m.used_kb)} / ${mb(m.available_kb)}</td><td>${mb(m.total_kb)}</td>`
		memBody.appendChild(tr)
		memPerc.push(m.used_percent)
		memTot.push(m.total_kb)
		memUsed.push(mb(m.used_kb))
		memAvail.push(mb(m.available_kb))
	})
	const memMin = Math.min(...memPerc)
	const memMax = Math.max(...memPerc)
	const memAvg = memPerc.reduce((a,b)=>a+b,0)/memPerc.length
	const memFinal = memPerc[memPerc.length-1
	]
	const li = snapshots.length-1
	document.getElementById("mem-agg-pct").textContent = `${f2(memMin)}% / ${f2(memMax)}% / ${f2(memAvg)}% / ${f2(memFinal)}%`
	document.getElementById("mem-agg-used-avail").textContent = `${memUsed[li]} / ${memAvail[li]} (последний)`
	document.getElementById("mem-agg-total").textContent = `${mb(memTot[li])}`

	const cpuBody = document.querySelector("#cpu-tbl tbody")
	const cpuPerc = [], l1 = [], l5 = [], l15 = [], lp = []
	snapshots.forEach(s => {
		const c = s.metrics.cpu, l = s.metrics.load
		const tr = document.createElement("tr")
		tr.innerHTML = `<td>${s.iteration}</td><td>${textTime(s.timestamp)}</td><td>${f2(c.cpu_usage_percent)}%</td><td>${f2(l.load_average_1min)} / ${f2(l.load_average_5min)} / ${f2(l.load_average_15min)}</td><td>${f2(l.load_percent_1min)}%</td>`
		cpuBody.appendChild(tr)
		cpuPerc.push(c.cpu_usage_percent)
		l1.push(l.load_average_1min)
		l5.push(l.load_average_5min)
		l15.push(l.load_average_15min)
		lp.push(l.load_percent_1min)
	})
	const cpuMin = Math.min(...cpuPerc)
	const cpuMax = Math.max(...cpuPerc)
	const cpuAvg = cpuPerc.reduce((a,b)=>a+b,0)/cpuPerc.length
	const cpuFinal = cpuPerc[cpuPerc.length-1]
	const l1min = Math.min(...l1), l1max = Math.max(...l1)
	const l5min = Math.min(...l5), l5max = Math.max(...l5)
	const l15min = Math.min(...l15), l15max = Math.max(...l15)
	const lpMin = Math.min(...lp), lpMax = Math.max(...lp)
	const lpAvg = lp.reduce((a,b)=>a+b,0)/lp.length
	const lpFinal = lp[lp.length-1]
	document.getElementById("cpu-agg-pct").textContent = `${f2(cpuMin)}% / ${f2(cpuMax)}% / ${f2(cpuAvg)}% / ${f2(cpuFinal)}%`
	document.getElementById("cpu-agg-load").textContent = `1min: ${f2(l1min)}–${f2(l1max)} | 5min: ${f2(l5min)}–${f2(l5max)} | 15min: ${f2(l15min)}–${f2(l15max)}`
	document.getElementById("cpu-agg-loadpct").textContent = `${f2(lpMin)}% / ${f2(lpMax)}% / ${f2(lpAvg)}% / ${f2(lpFinal)}%`

	const swapBody = document.querySelector("#swap-tbl tbody")
	const swPerc = [], swUsed = [], swTot = []
	snapshots.forEach(s => {
		const sw = s.metrics.swap
		const tr = document.createElement("tr")
		tr.innerHTML = `<td>${s.iteration}</td><td>${textTime(s.timestamp)}</td><td>${f2(sw.used_percent)}%</td><td>${mb(sw.used_kb)} / ${mb(sw.total_kb)}</td>`
		swapBody.appendChild(tr)
		swPerc.push(sw.used_percent)
		swUsed.push(mb(sw.used_kb))
		swTot.push(mb(sw.total_kb))
	})
	const swMin = Math.min(...swPerc)
	const swMax = Math.max(...swPerc)
	const swAvg = swPerc.reduce((a,b)=>a+b,0)/swPerc.length
	const swFinal = swPerc[swPerc.length-1]
	document.getElementById("swap-agg-pct").textContent = `${f2(swMin)}% / ${f2(swMax)}% / ${f2(swAvg)}% / ${f2(swFinal)}%`
	document.getElementById("swap-agg-used-total").textContent = `${swUsed[swUsed.length-1]} / ${swTot[swTot.length-1]} (последний)`

	const evBody = document.querySelector("#events-tbl tbody")
	const typeCnt = new Map()
	let firstAny = null
	snapshots.forEach(s => {
		const arr = s.metrics.critical_events || []
		arr.forEach(ev => {
			const tr = document.createElement("tr")
			tr.innerHTML = `<td>${s.iteration}</td><td>${textTime(s.timestamp)}</td><td>${ev.event_type}</td><td>${ev.severity}</td><td>${ev.description}</td>`
			evBody.appendChild(tr)
			typeCnt.set(ev.event_type, (typeCnt.get(ev.event_type) || 0) + 1)
			if (!firstAny) firstAny = { it:s.iteration, time:textTime(s.timestamp), type:ev.event_type, desc:ev.description }
		})
	})
	if (typeCnt.size === 0) {
		const tr = document.createElement("tr")
		tr.innerHTML = `<td colspan="5">—</td>`
		evBody.appendChild(tr)
	}
	const summary = [...typeCnt.entries()].map(([k,v]) => `${k}: ${v}`).join(" | ") || "—"
	document.getElementById("events-agg-types").textContent = summary

	const memBadges = document.getElementById("mem-badges")
	if (memBadges) {
		memBadges.innerHTML = ""
		memBadges.appendChild(badgeEl(`min ${f2(memMin)}%`, clsMem(memMin)))
		memBadges.appendChild(badgeEl(`max ${f2(memMax)}%`, clsMem(memMax)))
		memBadges.appendChild(badgeEl(`avg ${f2(memAvg)}%`, clsMem(memAvg)))
		memBadges.appendChild(badgeEl(`final ${f2(memFinal)}%`, clsMem(memFinal)))
	}
	const memFirstCrit = (() => {
		for (const s of snapshots) { const v = s.metrics.memory.used_percent; if (v > 85) return { it:s.iteration, time:textTime(s.timestamp), val:v } }
		return null
	})()
	const memFirst = document.getElementById("mem-first")
	if (memFirst) memFirst.textContent = memFirstCrit ? `Первое превышение CRITICAL: ${memFirstCrit.time} (#${memFirstCrit.it}, ${f2(memFirstCrit.val)}%)` : "Превышений порогов нет"

	const cpuBadges = document.getElementById("cpu-badges")
	if (cpuBadges) {
		cpuBadges.innerHTML = ""
		cpuBadges.appendChild(badgeEl(`CPU avg ${f2(cpuAvg)}%`, clsCpu(cpuAvg)))
		cpuBadges.appendChild(badgeEl(`CPU max ${f2(cpuMax)}%`, clsCpu(cpuMax)))
		cpuBadges.appendChild(badgeEl(`Load1 avg ${f2(lpAvg)}%`, clsLoad(lpAvg)))
		cpuBadges.appendChild(badgeEl(`Load1 max ${f2(lpMax)}%`, clsLoad(lpMax)))
	}
	const loadFirstCrit = (() => {
		for (const s of snapshots) { const v = s.metrics.load.load_percent_1min; if (v > 250) return { it:s.iteration, time:textTime(s.timestamp), val:v } }
		return null
	})()
	const cpuFirst = document.getElementById("cpu-first")
	if (cpuFirst) cpuFirst.textContent = loadFirstCrit ? `Первое превышение CRITICAL по Load1%: ${loadFirstCrit.time} (#${loadFirstCrit.it}, ${f2(loadFirstCrit.val)}%)` : "Превышений порогов нет"

	const swapBadges = document.getElementById("swap-badges")
	if (swapBadges) {
		swapBadges.innerHTML = ""
		swapBadges.appendChild(badgeEl(`avg ${f2(swAvg)}%`, clsSwap(swAvg)))
		swapBadges.appendChild(badgeEl(`max ${f2(swMax)}%`, clsSwap(swMax)))
		swapBadges.appendChild(badgeEl(`final ${f2(swFinal)}%`, clsSwap(swFinal)))
	}
	const swapFirstCrit = (() => {
		for (const s of snapshots) { const v = s.metrics.swap.used_percent; if (v > 60) return { it:s.iteration, time:textTime(s.timestamp), val:v } }
		return null
	})()
	const swapFirst = document.getElementById("swap-first")
	if (swapFirst) swapFirst.textContent = swapFirstCrit ? `Первое превышение CRITICAL: ${swapFirstCrit.time} (#${swapFirstCrit.it}, ${f2(swapFirstCrit.val)}%)` : "Превышений порогов нет"

	const evBadges = document.getElementById("ev-badges")
	if (evBadges) {
		evBadges.innerHTML = ""
		const mCnt = typeCnt.get("MEMORY_CRITICAL") || 0
		const sCnt = typeCnt.get("SWAP_CRITICAL") || 0
		evBadges.appendChild(badgeEl(`MEMORY_CRITICAL: ${mCnt}`, mCnt ? "crit" : "ok"))
		evBadges.appendChild(badgeEl(`SWAP_CRITICAL: ${sCnt}`, sCnt ? "crit" : "ok"))
	}
	const evFirst = document.getElementById("ev-first")
	if (evFirst) evFirst.textContent = firstAny ? `Первое критическое событие: ${firstAny.time} (#${firstAny.it}, ${firstAny.type})` : "События отсутствуют"

	fillScoreTable()
}

function init() {
	setCode()
	const node = document.getElementById("embedded-json")
	if (node && node.textContent.trim().length) {
		try { const data = JSON.parse(node.textContent.trim()); renderSnapshots(data); return } catch (_) {}
	}
	if (location.protocol === "http:" || location.protocol === "https:") {
		fetch("snapshots_incremental.json")
			.then(r => { if (!r.ok) throw new Error("HTTP " + r.status); return r.json() })
			.then(data => renderSnapshots(data))
			.catch(() => {
				const gen = document.getElementById("generated-at")
				if (gen) gen.textContent = "ошибка загрузки данных"
				fillScoreTable()
			})
		return
	}
	const gen = document.getElementById("generated-at")
	if (gen) gen.textContent = "данные отсутствуют"
	fillScoreTable()
}

document.addEventListener("DOMContentLoaded", init)
document.addEventListener("DOMContentLoaded", () => {
	const btn = document.getElementById("btn-json");
	if (btn) {
		btn.addEventListener("click", () => {
			window.open("https://github.com/hazed7/final_assgnm_linux/blob/main/snapshots_incremental.json", "_blank");
		});
	}
});
