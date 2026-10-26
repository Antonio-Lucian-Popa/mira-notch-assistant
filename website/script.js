/* Mira landing — light interactions. No dependencies. */
(() => {
  'use strict';
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];

  /* ---- mobile nav ---- */
  const burger = $('#hamburger');
  const links = $('#navLinks');
  if (burger && links) {
    burger.addEventListener('click', () => {
      const open = links.classList.toggle('open');
      burger.setAttribute('aria-expanded', String(open));
    });
    $$('#navLinks a').forEach(a => a.addEventListener('click', () => {
      links.classList.remove('open');
      burger.setAttribute('aria-expanded', 'false');
    }));
  }

  /* ---- scroll reveal ---- */
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
    }
  }, { threshold: 0.12, rootMargin: '0px 0px -40px 0px' });
  $$('.reveal').forEach(el => io.observe(el));

  /* ---- live clock in the hero notch ---- */
  const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const MON = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const dEl = $('#demoDate'), tEl = $('#demoTime');
  function tick() {
    const n = new Date();
    if (dEl) dEl.textContent = `${DAYS[n.getDay()]} ${n.getDate()} ${MON[n.getMonth()]} ${n.getFullYear()}`;
    if (tEl) tEl.textContent = `${String(n.getHours()).padStart(2, '0')}:${String(n.getMinutes()).padStart(2, '0')}`;
  }
  tick(); setInterval(tick, 1000 * 20);

  /* ---- eyes follow the cursor (the signature feature, on the page) ---- */
  const groups = [
    { wrap: $('#mascotChip'), eyes: $$('#heroEyes i'), max: 3.5 },
    { wrap: $('#bigEyes'), eyes: $$('#bigEyes i'), max: 7 },
  ].filter(g => g.wrap && g.eyes.length);

  let px = innerWidth / 2, py = innerHeight / 2, hasPointer = false;
  addEventListener('pointermove', (e) => { px = e.clientX; py = e.clientY; hasPointer = true; }, { passive: true });

  function follow() {
    for (const g of groups) {
      const r = g.wrap.getBoundingClientRect();
      const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
      let ox = 0, oy = 0;
      if (hasPointer) {
        ox = Math.max(-1, Math.min(1, (px - cx) / 180)) * g.max;
        oy = Math.max(-1, Math.min(1, (py - cy) / 180)) * g.max;
      }
      for (const eye of g.eyes) eye.style.transform = `translate(${ox}px, ${oy}px)`;
    }
    requestAnimationFrame(follow);
  }
  requestAnimationFrame(follow);

  /* occasional blink on the big eyes */
  const big = $('#bigEyes');
  if (big) setInterval(() => {
    big.querySelectorAll('i').forEach(i => {
      i.style.transition = 'transform .09s';
      const t = i.style.transform;
      i.style.transform = t + ' scaleY(.12)';
      setTimeout(() => { i.style.transform = t; i.style.transition = 'transform .12s ease-out'; }, 110);
    });
  }, 4200);

  /* ---- screens tabs ---- */
  $$('.tab').forEach(tab => {
    tab.addEventListener('click', () => {
      const name = tab.dataset.screen;
      $$('.tab').forEach(t => t.classList.toggle('is-active', t === tab));
      $$('.mock').forEach(m => m.classList.toggle('is-active', m.dataset.screen === name));
    });
  });

  /* ---- mascot buttons ---- */
  const winkBtn = $('#btnWink');
  if (winkBtn && big) winkBtn.addEventListener('click', () => {
    big.classList.add('love');
    setTimeout(() => big.classList.remove('love'), 1400);
  });

  /* ---- confetti ---- */
  const canvas = $('#confetti');
  const ctx = canvas && canvas.getContext('2d');
  let parts = [], raf = 0;
  function resize() { if (!canvas) return; canvas.width = innerWidth; canvas.height = innerHeight; }
  resize(); addEventListener('resize', resize);
  const COLORS = ['#ff5a5a', '#ffb24d', '#ffe24d', '#2ee67a', '#3b9cff', '#9a6bff', '#ff7aa6', '#ffffff'];

  function burst(n = 160) {
    if (!ctx) return;
    const originX = innerWidth / 2;
    for (let i = 0; i < n; i++) {
      parts.push({
        x: originX + (Math.random() * 200 - 100),
        y: innerHeight * 0.32,
        vx: (Math.random() - 0.5) * 9,
        vy: Math.random() * -12 - 4,
        g: 0.28 + Math.random() * 0.12,
        size: 5 + Math.random() * 6,
        rot: Math.random() * Math.PI,
        vr: (Math.random() - 0.5) * 0.3,
        color: COLORS[(Math.random() * COLORS.length) | 0],
        life: 1,
      });
    }
    if (!raf) raf = requestAnimationFrame(draw);
  }

  function draw() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    parts = parts.filter(p => p.life > 0 && p.y < innerHeight + 40);
    for (const p of parts) {
      p.vy += p.g; p.x += p.vx; p.y += p.vy; p.rot += p.vr; p.vx *= 0.99;
      if (p.y > innerHeight * 0.6) p.life -= 0.012;
      ctx.save();
      ctx.globalAlpha = Math.max(0, p.life);
      ctx.translate(p.x, p.y); ctx.rotate(p.rot);
      ctx.fillStyle = p.color;
      ctx.fillRect(-p.size / 2, -p.size / 2, p.size, p.size * 0.6);
      ctx.restore();
    }
    if (parts.length) { raf = requestAnimationFrame(draw); }
    else { ctx.clearRect(0, 0, canvas.width, canvas.height); raf = 0; }
  }

  const confettiBtn = $('#btnConfetti');
  if (confettiBtn) confettiBtn.addEventListener('click', () => {
    burst(180);
    if (big) { big.classList.add('love'); setTimeout(() => big.classList.remove('love'), 1600); }
  });

  /* a tiny welcome burst the first time the mascot section is seen */
  const mascotSection = $('#mascot');
  if (mascotSection) {
    const once = new IntersectionObserver((en) => {
      if (en[0].isIntersecting) { burst(90); once.disconnect(); }
    }, { threshold: 0.5 });
    once.observe(mascotSection);
  }
})();
