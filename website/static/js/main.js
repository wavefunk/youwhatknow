// Reveal animations (intersection observer)
(function () {
  var observer = new IntersectionObserver(
    function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add("visible");
        }
      });
    },
    { threshold: 0.15 }
  );
  document.querySelectorAll(".reveal").forEach(function (el) {
    observer.observe(el);
  });
})();

// Terminal demo animation (only on landing page)
(function () {
  var terminal = document.getElementById("terminal-demo");
  if (!terminal) return;

  var lines = [
    { type: "cmd", prompt: "claude", cmd: "Read", file: "src/server.rs" },
    { type: "msg", text: "  623 lines.", cls: "t-dim" },
    { type: "msg", text: "", cls: "t-dim" },
    { type: "msg", text: "  youwhatknow: denied.", cls: "t-ok t-bold" },
    {
      type: "msg",
      text: "  src/server.rs (623 lines) \u2014 Axum server with activity tracking",
      cls: "t-ok",
    },
    {
      type: "msg",
      text: "  Public: create_router, start_server, AppState",
      cls: "t-ok",
    },
    {
      type: "msg",
      text: "  If this summary is sufficient, do not read the file.",
      cls: "t-ok",
    },
    { type: "msg", text: "", cls: "t-dim" },
    { type: "cmd", prompt: "claude", cmd: "Read", file: "src/config.rs" },
    {
      type: "msg",
      text: "  Claude moves on. Summary was enough.",
      cls: "t-dim",
    },
    { type: "msg", text: "", cls: "t-dim" },
    { type: "cmd", prompt: "claude", cmd: "Read", file: "src/server.rs" },
    { type: "msg", text: "  Second read \u2014 allowed through.", cls: "t-dim" },
    { type: "msg", text: "", cls: "t-dim" },
    { type: "cmd", prompt: "claude", cmd: "Read", file: "src/server.rs" },
    { type: "msg", text: "  Third time. Really?", cls: "t-warn" },
    {
      type: "msg",
      text: "  read 3x this session \u2014 consider offset/limit",
      cls: "t-ok",
    },
  ];

  var lineIndex = 0;

  function addLine() {
    if (lineIndex >= lines.length) {
      var cursor = document.createElement("span");
      cursor.className = "cursor-blink";
      if (terminal.lastElementChild) {
        terminal.lastElementChild.appendChild(cursor);
      }
      return;
    }

    var line = lines[lineIndex];
    var el = document.createElement("div");
    el.className = "term-line";

    if (line.type === "cmd") {
      var prompt = document.createElement("span");
      prompt.className = "t-prompt";
      prompt.textContent = line.prompt;
      var cmd = document.createElement("span");
      cmd.className = "t-cmd";
      cmd.textContent = line.cmd;
      var file = document.createElement("span");
      file.className = "t-file";
      file.textContent = line.file;
      el.appendChild(prompt);
      el.appendChild(document.createTextNode(" "));
      el.appendChild(cmd);
      el.appendChild(document.createTextNode(" "));
      el.appendChild(file);
    } else {
      var span = document.createElement("span");
      span.className = line.cls;
      span.textContent = line.text;
      el.appendChild(span);
    }

    terminal.appendChild(el);
    lineIndex++;

    var delay = line.text === "" ? 500 : lineIndex > 10 ? 150 : 300;
    setTimeout(addLine, delay);
  }

  setTimeout(addLine, 1000);
})();

// Sidebar active section tracking (only on docs page)
(function () {
  var sidebarLinks = document.querySelectorAll(".sidebar-nav a");
  if (!sidebarLinks.length) return;

  var sections = [];
  sidebarLinks.forEach(function (link) {
    var href = link.getAttribute("href");
    if (href && href.startsWith("#")) {
      var section = document.getElementById(href.slice(1));
      if (section) sections.push({ el: section, link: link });
    }
  });

  var observer = new IntersectionObserver(
    function (entries) {
      entries.forEach(function (entry) {
        var match = sections.find(function (s) {
          return s.el === entry.target;
        });
        if (match) {
          if (entry.isIntersecting) {
            sidebarLinks.forEach(function (l) {
              l.classList.remove("active");
            });
            match.link.classList.add("active");
          }
        }
      });
    },
    { rootMargin: "-20% 0px -70% 0px" }
  );

  sections.forEach(function (s) {
    observer.observe(s.el);
  });
})();

// Copy buttons for code blocks in docs
document.addEventListener("DOMContentLoaded", function () {
  document.querySelectorAll(".doc-content pre").forEach(function (pre) {
    var btn = document.createElement("button");
    btn.className = "copy-btn";
    btn.textContent = "copy";
    btn.addEventListener("click", function () {
      var code = pre.querySelector("code");
      navigator.clipboard.writeText(code ? code.textContent : pre.textContent);
      btn.textContent = "copied";
      setTimeout(function () {
        btn.textContent = "copy";
      }, 1500);
    });
    pre.appendChild(btn);
  });
});
