<?xml version="1.0" encoding="EUC-KR"?>
<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
  <xsl:template match="/">
    <html lang="en">
      <body>
        <h1>빵집에 오신 것을 환영합니다</h1>
        <p>주문이 <b>도착했습니다</b> <xsl:value-of select="count(item)"/> 개.</p>
        <img src="{@src}" alt="갓 구운 빵 사진"/>
        <xsl:if test="a"><xsl:text>저장 후 닫기</xsl:text></xsl:if>
        <p translate="no">ACME-42</p>
      </body>
    </html>
  </xsl:template>
</xsl:stylesheet>
