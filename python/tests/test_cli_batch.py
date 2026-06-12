from __future__ import annotations

import pytest

from cs2df import cli


class NativeParserFailure(BaseException):
    pass


def test_export_one_report_converts_native_failures(monkeypatch, tmp_path):
    dem = tmp_path / "bad.dem"
    dem.write_bytes(b"not a demo")

    def fail_export(*args, **kwargs):
        raise NativeParserFailure("native parser panic")

    import cs2df.package

    monkeypatch.setattr(cs2df.package, "export_demo", fail_export)

    row = cli._export_one_report(str(dem), str(tmp_path), {}, descriptive=False)

    assert row["ok"] is False
    assert row["zip"] is None
    assert row["demoBytes"] == len(b"not a demo")
    assert row["error"] == "NativeParserFailure: native parser panic"


def test_write_unique_zip_does_not_overwrite_descriptive_collisions(tmp_path):
    first = cli._write_unique_zip(tmp_path, "2026-06-12_de_mirage_A-vs-B_13-11.zip",
                                  "match-one", b"first")
    second = cli._write_unique_zip(tmp_path, "2026-06-12_de_mirage_A-vs-B_13-11.zip",
                                   "match-two", b"second")

    assert first.name == "2026-06-12_de_mirage_A-vs-B_13-11.zip"
    assert second.name == "2026-06-12_de_mirage_A-vs-B_13-11_match-two.zip"
    assert first.read_bytes() == b"first"
    assert second.read_bytes() == b"second"


def test_export_one_report_keeps_plain_batch_overwrite(monkeypatch, tmp_path):
    dem = tmp_path / "match.dem"
    dem.write_bytes(b"demo")
    existing = tmp_path / "match.zip"
    existing.write_bytes(b"old")

    seen_kwargs = {}

    def export(*args, **kwargs):
        seen_kwargs.update(kwargs)
        return b"new", {
            "mapName": "de_mirage",
            "teamA": {},
            "teamB": {},
            "compressLevel": 3,
            "timingsSeconds": {"parse.total": 1.2, "package.writeZip": 0.3},
        }

    import cs2df.package

    monkeypatch.setattr(cs2df.package, "export_demo", export)

    row = cli._export_one_report(str(dem), str(tmp_path), {"compress_level": 3}, descriptive=False)

    assert row["ok"] is True
    assert row["zip"] == "match.zip"
    assert existing.read_bytes() == b"new"
    assert seen_kwargs["compress_level"] == 3
    assert row["compressLevel"] == 3
    assert row["timingsSeconds"]["parse.total"] == 1.2
    assert "batch.writeFile" in row["timingsSeconds"]


def test_aggregate_timings_totals_and_averages():
    report = [
        {"ok": True, "timingsSeconds": {"parse.total": 2.0, "package.writeZip": 0.4}},
        {"ok": True, "timingsSeconds": {"parse.total": 4.0, "package.writeZip": 0.8}},
        {"ok": False, "timingsSeconds": None},
    ]

    aggregate = cli._aggregate_timings(report)

    assert aggregate == {
        "count": 2,
        "total": {"package.writeZip": 1.2, "parse.total": 6.0},
        "average": {"package.writeZip": 0.6, "parse.total": 3.0},
    }


def test_compress_level_bounds():
    assert cli._compress_level_ok(0)
    assert cli._compress_level_ok(9)
    assert not cli._compress_level_ok(-1)
    assert not cli._compress_level_ok(10)


def test_future_failure_is_reported_as_batch_row(tmp_path):
    dem = tmp_path / "bad.dem"
    dem.write_bytes(b"x")
    started = 0.0

    row = cli._failed_batch_row(dem, started, cli._format_exception(RuntimeError("worker died")))

    assert row["ok"] is False
    assert row["zip"] is None
    assert row["error"] == "RuntimeError: worker died"
    assert row["demoBytes"] == 1
    assert row["compressLevel"] is None
    assert row["timingsSeconds"] is None


def test_export_one_report_preserves_interrupts(monkeypatch, tmp_path):
    dem = tmp_path / "interrupt.dem"
    dem.write_bytes(b"x")

    def fail_export(*args, **kwargs):
        raise KeyboardInterrupt

    import cs2df.package

    monkeypatch.setattr(cs2df.package, "export_demo", fail_export)

    with pytest.raises(KeyboardInterrupt):
        cli._export_one_report(str(dem), str(tmp_path), {}, descriptive=False)
