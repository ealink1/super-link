package application

import "io"

// Reports buffered input reads even while a single large statement is scanned.
type sqlFileProgressReader struct {
	io.Reader
	bytes  int64
	update func(int64)
}

func (r *sqlFileProgressReader) Read(p []byte) (int, error) {
	n, err := r.Reader.Read(p)
	r.bytes += int64(n)
	if n > 0 {
		r.update(r.bytes)
	}
	return n, err
}
