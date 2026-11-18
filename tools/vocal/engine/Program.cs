// Pro Wave's headless singer. Reads a score (JSON) of phrases, renders each phrase with
// OpenUtau.Core's DiffSinger runtime and writes one mono 32-bit float WAV per phrase.
//
//   prowave-sing score.json
//
// Output lines on stdout: "STEP <text>" while working, then one JSON line at the end:
//   {"ok":true,"phrases":[{"id":"...","file":"...","secs":1.23}],"render_secs":4.5}
// Sample 0 of each WAV is time 0 of its phrase (the score's note times are from there).
//
// OpenUtau keeps its data under $HOME/Library/OpenUtau; Pro Wave runs this with HOME set to
// its own vocal folder (`vocal` in Pro Wave's settings folder), so the voices and caches stay there.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using NAudio.Wave;
using NAudio.Wave.SampleProviders;
using OpenUtau.Classic;
using OpenUtau.Core;
using OpenUtau.Core.Format;
using OpenUtau.Core.Render;
using OpenUtau.Core.SignalChain;
using OpenUtau.Core.Ustx;
using OpenUtau.Core.Util;
using Serilog;

namespace ProwaveSing {
    /// The "UI thread" OpenUtau's document code expects: one thread running posted tasks in order.
    sealed class LoopScheduler : TaskScheduler {
        readonly BlockingCollection<Task> queue = new BlockingCollection<Task>();
        public readonly Thread Thread;
        public LoopScheduler() {
            Thread = new Thread(() => {
                foreach (var t in queue.GetConsumingEnumerable()) {
                    TryExecuteTask(t);
                }
            }) { IsBackground = true, Name = "prowave-sing-main" };
            Thread.Start();
        }
        protected override void QueueTask(Task task) => queue.Add(task);
        protected override bool TryExecuteTaskInline(Task task, bool prev) =>
            Thread.CurrentThread == Thread && TryExecuteTask(task);
        protected override IEnumerable<Task> GetScheduledTasks() => queue.ToArray();
        public void Run(Action a) =>
            Task.Factory.StartNew(a, CancellationToken.None, TaskCreationOptions.None, this).GetAwaiter().GetResult();
        public T Run<T>(Func<T> f) =>
            Task.Factory.StartNew(f, CancellationToken.None, TaskCreationOptions.None, this).GetAwaiter().GetResult();
    }

    sealed class Note {
        public double start { get; set; }
        public double dur { get; set; }
        public int pitch { get; set; }
        public string lyric { get; set; } = "a";
    }

    sealed class Phrase {
        public string id { get; set; } = "phrase";
        public string out_file { get; set; } = "";
        public List<Note> notes { get; set; } = new List<Note>();
    }

    sealed class Score {
        public string singer { get; set; } = "";
        public string phonemizer { get; set; } = "";
        public string language { get; set; } = "en";
        public string color { get; set; } = "";
        public double gender { get; set; }
        public double breathiness { get; set; }
        public double tension { get; set; }
        public double voicing { get; set; } = 100;
        public double velocity { get; set; } = 100;
        public double vibrato_depth { get; set; } = 25;
        public double vibrato_rate { get; set; } = 5.5;
        public double vibrato_length { get; set; } = 60;
        public double vibrato_min_secs { get; set; } = 0.35;
        public double transition_ms { get; set; } = 80;
        public bool list_singers { get; set; }
        public List<Phrase> phrases { get; set; } = new List<Phrase>();
    }

    static class Program {
        // The scores are in seconds; at 120 BPM a tick is 1/960 s.
        const double TicksPerSec = 960.0;

        static int Tick(double secs) => (int)Math.Round(secs * TicksPerSec);

        static void Step(string s) {
            Console.WriteLine("STEP " + s);
            Console.Out.Flush();
        }

        static string PhonemizerFor(string language) => (language ?? "en").ToLowerInvariant() switch {
            "ja" => "OpenUtau.Core.DiffSinger.DiffSingerJapanesePhonemizer",
            "zh" => "OpenUtau.Core.DiffSinger.DiffSingerChinesePhonemizer",
            "ko" => "OpenUtau.Core.DiffSinger.DiffSingerKoreanPhonemizer",
            "es" => "OpenUtau.Core.DiffSinger.DiffSingerSpanishPhonemizer",
            "de" => "OpenUtau.Core.DiffSinger.DiffSingerGermanPhonemizer",
            "fr" => "OpenUtau.Core.DiffSinger.DiffSingerFrenchPhonemizer",
            _ => "OpenUtau.Core.DiffSinger.DiffSingerEnglishPhonemizer",
        };

        static UProject Build(Score score, Phrase phrase, USinger singer) {
            var project = Ustx.Create();
            project.tempos = new List<UTempo> { new UTempo(0, 120) };
            project.timeSignatures = new List<UTimeSignature> { new UTimeSignature(0, 4, 4) };
            var track = project.tracks[0];
            track.TrackName = "Vocal";
            track.singer = singer.Id;
            track.phonemizer = string.IsNullOrEmpty(score.phonemizer) ? PhonemizerFor(score.language) : score.phonemizer;
            var part = new UVoicePart() { name = phrase.id, trackNo = 0, position = 0 };
            int end = 0;
            foreach (var n in phrase.notes.OrderBy(n => n.start)) {
                int pos = Tick(n.start);
                int dur = Math.Max(15, Tick(n.start + n.dur) - pos);
                var note = project.CreateNote(n.pitch, pos, dur);
                note.lyric = string.IsNullOrWhiteSpace(n.lyric) ? "a" : n.lyric.Trim();
                // Pitch transition: the glide from the note before, centred on the note start.
                int glide = (int)Math.Clamp(score.transition_ms, 5, 500);
                note.pitch.data[0].X = -glide / 2f;
                note.pitch.data[1].X = glide / 2f;
                // Vibrato on held notes only.
                if (score.vibrato_depth > 0 && n.dur >= score.vibrato_min_secs) {
                    note.vibrato.length = (float)Math.Clamp(score.vibrato_length, 0, 100);
                    note.vibrato.period = (float)(1000.0 / Math.Clamp(score.vibrato_rate, 2, 12));
                    note.vibrato.depth = (float)Math.Clamp(score.vibrato_depth, 5, 200);
                    note.vibrato.@in = 20;
                    note.vibrato.@out = 10;
                } else {
                    note.vibrato.length = 0;
                }
                part.notes.Add(note);
                end = Math.Max(end, pos + dur);
            }
            part.Duration = end + Tick(1.0);
            void Curve(string abbr, double value, double neutral) {
                if (Math.Abs(value - neutral) < 1e-6 || !project.expressions.TryGetValue(abbr, out var d)) {
                    return;
                }
                var c = new UCurve(d);
                int v = (int)Math.Round(Math.Clamp(value, d.min, d.max));
                c.xs.AddRange(new[] { 0, part.Duration });
                c.ys.AddRange(new[] { v, v });
                part.curves.Add(c);
            }
            Curve(Ustx.GENC, score.gender, 0);
            Curve(Ustx.BREC, score.breathiness, 0);
            Curve(Ustx.TENC, score.tension, 0);
            Curve(Ustx.VOIC, score.voicing, 100);
            if (project.expressions.TryGetValue(Ustx.VEL, out var vel)) {
                vel.CustomDefaultValue = (float)Math.Clamp(score.velocity, 0, 200);
            }
            project.parts.Add(part);
            return project;
        }

        static void WriteMonoFloat(string path, ISampleProvider stereo) {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path)));
            var tmp = path + ".part";
            WaveFileWriter.CreateWaveFile(tmp, new SampleToWaveProvider(stereo.ToMono(1, 0)));
            File.Move(tmp, path, true);
        }

        static int Main(string[] args) {
            if (args.Length < 1) {
                Console.Error.WriteLine("usage: prowave-sing score.json");
                return 2;
            }
            var total = Stopwatch.StartNew();
            // Voicebank files are often Shift-JIS, which .NET only reads with this provider.
            System.Text.Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);
            // OpenUtau's log goes to stderr (PROWAVE_SING_LOG=1 for everything, else warnings).
            Serilog.Log.Logger = new Serilog.LoggerConfiguration()
                .MinimumLevel.Is(Environment.GetEnvironmentVariable("PROWAVE_SING_LOG") == "1"
                    ? Serilog.Events.LogEventLevel.Information
                    : Serilog.Events.LogEventLevel.Warning)
                .WriteTo.Console(standardErrorFromLevel: Serilog.Events.LogEventLevel.Verbose)
                .CreateLogger();
            try {
                var score = JsonSerializer.Deserialize<Score>(File.ReadAllText(args[0]));
                var loop = new LoopScheduler();
                Step("loading voices");
                loop.Run(() => {
                    ToolsManager.Inst.Initialize();
                    SingerManager.Inst.Initialize();
                    DocManager.Inst.Initialize(loop.Thread, loop);
                    DocManager.Inst.PostOnUIThread = a =>
                        Task.Factory.StartNew(a, CancellationToken.None, TaskCreationOptions.None, loop);
                });
                var singers = SingerManager.Inst.Singers.Values
                    .Where(s => s.SingerType == USingerType.DiffSinger).ToList();
                if (score.list_singers) {
                    var list = SingerManager.Inst.Singers.Values.Select(s => new {
                        id = s.Id,
                        name = s.Name,
                        type = s.SingerType.ToString(),
                        location = s.Location,
                        colors = s.Subbanks.Select(b => b.Color).Distinct().OrderBy(c => c).ToArray(),
                    });
                    Console.WriteLine(JsonSerializer.Serialize(new { ok = true, singers = list }));
                    return 0;
                }
                var singer = singers.FirstOrDefault(s => s.Id == score.singer)
                    ?? singers.FirstOrDefault(s => s.Name == score.singer)
                    ?? singers.FirstOrDefault(s => s.Id.Contains(score.singer ?? ""))
                    ?? singers.FirstOrDefault();
                if (singer == null) {
                    throw new Exception("no DiffSinger voice is installed");
                }
                var results = new List<object>();
                int k = 0;
                foreach (var phrase in score.phrases) {
                    k++;
                    if (phrase.notes.Count == 0) {
                        continue;
                    }
                    Step($"phrase {k}/{score.phrases.Count}");
                    var phraseClock = Stopwatch.StartNew();
                    var project = loop.Run(() => {
                        var p = Build(score, phrase, singer);
                        DocManager.Inst.TakeProjectForTest(p);
                        p.AfterLoad();
                        p.ValidateFull();
                        return p;
                    });
                    // The phonemizer answers on its own thread and posts back to the loop.
                    // (Its WaitFinish() never returns once the runner is idle: the runner waits
                    // for work while holding the lock WaitFinish takes, so poll the part.)
                    var part = (UVoicePart)project.parts[0];
                    var waited = Stopwatch.StartNew();
                    while (waited.Elapsed.TotalSeconds < 120
                        && !loop.Run(() => part.PhonemesUpToDate && part.phonemes.Count > 0)) {
                        Thread.Sleep(10);
                    }
                    Log.Information($"prowave: phonemized in {waited.Elapsed.TotalSeconds:F2}s");
                    if (part.phonemes.Count == 0) {
                        throw new Exception($"the phonemizer gave no phonemes for phrase {phrase.id}");
                    }
                    loop.Run(() => {
                        var track = project.tracks[0];
                        if (track.VoiceColorExp != null && !string.IsNullOrEmpty(score.color)) {
                            int i = Array.FindIndex(track.VoiceColorExp.options, o => o == score.color || o.Contains(score.color));
                            if (i >= 0) {
                                track.VoiceColorExp.CustomDefaultValue = i;
                            }
                        }
                        project.Validate(new ValidateOptions { SkipPhonemizer = true });
                    });
                    Log.Information("prowave: validated, rendering");
                    var engine = new RenderEngine(project);
                    var planner = new MixPlanner();
                    CancellationTokenSource cancel = null;
                    var mixes = engine.RenderTracks(loop, ref cancel, planner);
                    Log.Information("prowave: rendered");
                    var mix = mixes.FirstOrDefault(m => m != null);
                    if (mix == null) {
                        throw new Exception($"nothing rendered for phrase {phrase.id}");
                    }
                    var adapter = new ExportAdapter(mix);
                    WriteMonoFloat(phrase.out_file, adapter);
                    double secs = 0;
                    using (var r = new WaveFileReader(phrase.out_file)) {
                        secs = r.TotalTime.TotalSeconds;
                    }
                    results.Add(new { id = phrase.id, file = phrase.out_file, secs, render_secs = phraseClock.Elapsed.TotalSeconds });
                }
                Console.WriteLine(JsonSerializer.Serialize(new {
                    ok = true,
                    singer = singer.Id,
                    phrases = results,
                    render_secs = total.Elapsed.TotalSeconds,
                }));
                return 0;
            } catch (Exception e) {
                Console.Error.WriteLine(e.ToString());
                Console.WriteLine(JsonSerializer.Serialize(new { ok = false, error = e.Message }));
                return 1;
            }
        }
    }
}
